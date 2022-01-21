use anyhow::{Error, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

const BASE_URL: &str = "https://api.linode.com/v4/";

/// Minimal Linode API client, just what is needed for this app.
pub struct Client {
    client: reqwest::Client,
    token: String,
}

#[derive(Deserialize, Debug)]
pub struct DomainResponse {
    pub axfr_ips: Vec<String>,
    pub description: Option<String>,
    pub domain: String,
    pub expire_sec: u64,
    pub group: Option<String>,
    pub id: u64,
    pub master_ips: Vec<String>,
    pub refresh_sec: u64,
    pub retry_sec: u64,
    pub soa_email: String,
    pub status: String,
    pub tags: Vec<String>,
    pub ttl_sec: u64,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Deserialize, Debug)]
pub struct DomainRecordResponse {
    pub created: String,
    pub id: u64,
    pub name: String,
    pub port: Option<u64>,
    pub priority: Option<u64>,
    pub protocol: Option<String>,
    pub service: Option<String>,
    pub tag: Option<String>,
    pub target: String,
    pub ttl_sec: u64,
    #[serde(rename = "type")]
    pub type_: String,
    pub updated: String,
    pub weight: Option<u64>,
}

#[derive(Serialize, Debug)]
pub struct DomainRecordRequest {
    pub name: String,
    pub port: Option<u64>,
    pub priority: Option<u64>,
    pub protocol: Option<String>,
    pub service: Option<String>,
    pub tag: Option<String>,
    pub target: String,
    pub ttl_sec: u64,
    #[serde(rename = "type")]
    pub type_: String,
    pub weight: Option<u64>,
}

#[derive(Serialize, Debug)]
pub struct RdnsUpdateRequest {
    pub rdns: String,
}

#[derive(Deserialize, Debug)]
pub struct RdnsUpdateResponse {
    pub address: String,
    pub gateway: String,
    pub linode_id: u64,
    pub prefix: u64,
    pub public: bool,
    pub region: String,
    pub subnet_mask: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Deserialize, Debug)]
pub struct IpAddressResponse {
    pub address: String,
    pub gateway: Option<String>,
    pub linode_id: Option<u64>,
    pub prefix: u64,
    pub public: bool,
    pub rdns: Option<String>,
    pub region: String,
    pub subnet_mask: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Deserialize, Debug)]
pub struct LinodeResponse<T: DeserializeOwned> {
    #[serde(deserialize_with = "T::deserialize")]
    data: T,
    page: u64,
    pages: u64,
    #[allow(dead_code)]
    results: u64,
}

impl From<DomainRecordResponse> for DomainRecordRequest {
    fn from(record: DomainRecordResponse) -> Self {
        DomainRecordRequest {
            name: record.name,
            port: record.port,
            priority: record.priority,
            protocol: record.protocol,
            service: record.service,
            tag: record.tag,
            target: record.target,
            ttl_sec: record.ttl_sec,
            type_: record.type_,
            weight: record.weight,
        }
    }
}

impl Client {
    pub fn new(token: &str) -> Client {
        Client {
            client: reqwest::Client::new(),
            token: String::from(token),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", BASE_URL, path);
        self.client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.token))
    }

    async fn get_list<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        // TODO: handle pagination
        let response = self
            .request(reqwest::Method::GET, endpoint)
            .send()
            .await?
            .error_for_status()?
            .json::<LinodeResponse<T>>()
            .await?;
        if (response.page != 1) || (response.pages != 1) {
            return Err(Error::msg("Pagination is required but is not implemented"));
        }
        Ok(response.data)
    }

    async fn delete(&self, endpoint: &str) -> Result<()> {
        self
            .request(reqwest::Method::DELETE, endpoint)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn request_with_body<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        body: &B,
    ) -> Result<T> {
        let response = self
            .request(method, endpoint)
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?;
        Ok(response)
    }

    async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(&self, endpoint: &str, body: &B) -> Result<T> {
        self.request_with_body(reqwest::Method::POST, endpoint, body).await
    }

    async fn put<T: DeserializeOwned, B: Serialize + ?Sized>(&self, endpoint: &str, body: &B) -> Result<T> {
        self.request_with_body(reqwest::Method::PUT, endpoint, body).await
    }

    pub async fn get_domains(&self) -> Result<Vec<DomainResponse>> {
        self.get_list("domains").await
    }

    pub async fn get_domain_records(&self, domain_id: u64) -> Result<Vec<DomainRecordResponse>> {
        self.get_list(&format!("domains/{}/records", domain_id)).await
    }

    pub async fn create_domain_record(
        &self,
        domain_id: u64,
        domain_record: DomainRecordRequest,
    ) -> Result<DomainRecordResponse> {
        self.post(&format!("domains/{}/records", domain_id), &domain_record)
            .await
    }

    pub async fn delete_domain_record(
        &self,
        domain_id: u64,
        domain_record_id: u64,
    ) -> Result<()> {
        self.delete(&format!("domains/{}/records/{}", domain_id, domain_record_id)).await
    }

    pub async fn update_domain_record(
        &self,
        domain_id: u64,
        domain_record_id: u64,
        domain_record: DomainRecordRequest,
    ) -> Result<DomainRecordResponse> {
        self.put(
            &format!("domains/{}/records/{}", domain_id, domain_record_id),
            &domain_record,
        )
        .await
    }

    pub async fn update_rdns(&self, ip: IpAddr, fqdn: &str) -> Result<RdnsUpdateResponse> {
        self.put(
            &format!("networking/ips/{}", ip.to_string()),
            &RdnsUpdateRequest { rdns: fqdn.to_string() },
        )
        .await
    }

    pub async fn get_ip_addresses(&self) -> Result<Vec<IpAddressResponse>> {
        self.get_list("networking/ips").await
    }
}
