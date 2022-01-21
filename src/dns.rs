use crate::linode;
use anyhow::{Context, Result};
use std::net::IpAddr;
use std::str::FromStr;
use tracing::{debug, info, instrument};
use trust_dns_resolver::config::*;
use trust_dns_resolver::{Name, TokioAsyncResolver};

/// How long to wait for the new A record to appear
const DNS_PROPAGATION_DELAY: u64 = 60;
const DNS_RECORD_TTL: u64 = 5 * 60;

fn resolver() -> Result<TokioAsyncResolver> {
    let config = ResolverConfig::default();
    let mut opts = ResolverOpts::default();
    opts.use_hosts_file = false;
    let resolver = TokioAsyncResolver::tokio(config, opts)?;
    Ok(resolver)
}

async fn add_a_record(linode_api_token: &str, domain: &str, host_name: &str, ip_address: IpAddr) -> Result<()> {
    let client = linode::Client::new(linode_api_token);
    let domains = client.get_domains().await?;
    let domain = domains
        .iter()
        .find(|d| d.domain == domain)
        .context(format!("Could not find domain {} at Linode", domain))?;
    let records = client.get_domain_records(domain.id).await?;

    let addr_type = if ip_address.is_ipv4() { "A" } else { "AAAA" };
    let record = records
        .into_iter()
        .find(|r| r.name == host_name && r.type_ == addr_type);
    if let Some(record) = record {
        if record.target == ip_address.to_string() {
            info!("Forward DNS record already defined in Linode");
            return Ok(());
        }
        let record_id = record.id;
        let mut update_request = linode::DomainRecordRequest::from(record);
        update_request.target = ip_address.to_string();
        client
            .update_domain_record(domain.id, record_id, update_request)
            .await?;
    } else {
        let create_request = linode::DomainRecordRequest {
            name: host_name.to_string(),
            target: ip_address.to_string(),
            type_: addr_type.to_string(),
            priority: None,
            port: None,
            weight: None,
            ttl_sec: DNS_RECORD_TTL,
            service: None,
            protocol: None,
            tag: None,
        };
        client.create_domain_record(domain.id, create_request).await?;
    }
    Ok(())
}

async fn trigger_rptr_update(linode_api_token: &str, fqdn: &str, ip_address: IpAddr) -> Result<()> {
    let client = linode::Client::new(linode_api_token);

    let addresses = client.get_ip_addresses().await?;
    if addresses
        .into_iter()
        .any(|a| a.address == ip_address.to_string() && a.rdns == Some(fqdn.to_string()))
    {
        info!("Reverse DNS record already defined in Linode");
        return Ok(());
    }
    client.update_rdns(ip_address, fqdn).await?;
    Ok(())
}

async fn forward_lookup_check(resolver: &TokioAsyncResolver, fqdn: &str, ip: IpAddr) -> Result<()> {
    let forward_lookup = resolver.lookup_ip(Name::from_str(fqdn)?).await?;
    let ip_address = forward_lookup
        .iter()
        .next()
        .context("Could not find IP address in response")?;
    if ip_address == ip {
        Ok(())
    } else {
        Err(anyhow::anyhow!("IP address does not match"))
    }
}

async fn reverse_lookup_check(resolver: &TokioAsyncResolver, ip: IpAddr, fqdn: &str) -> Result<()> {
    let reverse_lookup = resolver.reverse_lookup(ip).await?;
    let name = reverse_lookup
        .into_iter()
        .next()
        .context("Could not find name in the response")?;
    if name == Name::from_str(fqdn)? {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Reverse lookup name does not match"))
    }
}

#[instrument(skip(linode_api_token))]
pub async fn update(linode_api_token: &str, domain: &str, host_name: &str, ip_address: IpAddr) -> Result<()> {
    debug!("Verifying forward and reverse DNS records");
    let resolver = resolver()?;
    let fqdn = format!("{}.{}", host_name, domain);

    if forward_lookup_check(&resolver, &fqdn, ip_address).await.is_err() {
        info!("Forward lookup failed, adding new DNS record");
        add_a_record(linode_api_token, &domain, &host_name, ip_address).await?;
        debug!(delay = DNS_PROPAGATION_DELAY, "Waiting for DNS propagation");
        tokio::time::sleep(std::time::Duration::from_secs(DNS_PROPAGATION_DELAY)).await;
    }
    if reverse_lookup_check(&resolver, ip_address, &fqdn).await.is_err() {
        info!("Reverse lookup failed, triggering API to update");
        trigger_rptr_update(linode_api_token, &fqdn, ip_address).await?;
        info!("PTR update was completed successfully")
    }
    Ok(())
}
