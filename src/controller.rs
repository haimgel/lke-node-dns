use crate::dns;
use crate::errors::Error;
use anyhow::{Context, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Node;
use kube::{
    api::{Api, ListParams},
    runtime::controller::{Context as ControllerContext, Controller, ReconcilerAction},
    runtime::finalizer::{finalizer, Event},
};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use tracing::{trace, warn};

lazy_static! {
    /// Keep a cache of the things we reconciled successfully, to prevent excessive DNS / API traffic.
    static ref CACHE: Mutex<HashMap<String, IpAddr>> = Mutex::new(HashMap::new());
}

/// Data we want access to in error/reconcile calls
struct ContextData {
    client: kube::Client,
    node_domain: String,
    linode_api_token: String,
}

struct NodeAddresses {
    host_name: String,
    ip_address: IpAddr,
}

impl TryFrom<Node> for NodeAddresses {
    type Error = Error;

    fn try_from(node: Node) -> std::result::Result<Self, Self::Error> {
        let addresses = node
            .status
            .as_ref()
            .ok_or(Error::MissingObjectKey(".status"))?
            .addresses
            .as_ref()
            .ok_or(Error::MissingObjectKey(".status.addresses"))?;
        let host_name = addresses
            .iter()
            .find(|address| address.type_ == "Hostname".to_string())
            .as_ref()
            .ok_or(Error::MissingObjectKey("status.addresses.Hostname"))?
            .address
            .as_str();
        let ip_address: IpAddr = addresses
            .iter()
            .find(|address| address.type_ == "ExternalIP".to_string())
            .as_ref()
            .ok_or(Error::MissingObjectKey("status.addresses.ExternalIP"))?
            .address
            .as_str()
            .parse()
            .context("ExternalIP is not a valid IPv4 address")?;
        Ok(NodeAddresses {
            host_name: host_name.to_string(),
            ip_address,
        })
    }
}

async fn apply(node: Node, ctx: ControllerContext<ContextData>) -> Result<ReconcilerAction, Error> {
    let node_addresses = NodeAddresses::try_from(node)?;
    if CACHE.lock().unwrap().get(node_addresses.host_name.as_str()) == Some(&node_addresses.ip_address) {
        return Ok(ReconcilerAction { requeue_after: None });
    }
    dns::update(
        ctx.get_ref().linode_api_token.as_str(),
        ctx.get_ref().node_domain.as_str(),
        node_addresses.host_name.as_str(),
        node_addresses.ip_address,
    )
    .await?;
    CACHE
        .lock()
        .unwrap()
        .insert(node_addresses.host_name, node_addresses.ip_address);
    Ok(ReconcilerAction { requeue_after: None })
}

async fn cleanup(node: Node, ctx: ControllerContext<ContextData>) -> Result<ReconcilerAction, Error> {
    let node_addresses = NodeAddresses::try_from(node)?;
    dns::delete(
        ctx.get_ref().linode_api_token.as_str(),
        ctx.get_ref().node_domain.as_str(),
        node_addresses.host_name.as_str(),
        node_addresses.ip_address,
    )
    .await?;
    CACHE.lock().unwrap().remove(node_addresses.host_name.as_str());
    Ok(ReconcilerAction { requeue_after: None })
}

async fn finalizer_reconcile(
    event: Event<Node>,
    ctx: ControllerContext<ContextData>,
) -> Result<ReconcilerAction, Error> {
    match event {
        Event::Apply(node) => Ok(apply(node, ctx).await?),
        Event::Cleanup(node) => Ok(cleanup(node, ctx).await?),
    }
}

/// Controller triggers this whenever any of the nodes have changed in any way
async fn reconcile(node: Node, ctx: ControllerContext<ContextData>) -> Result<ReconcilerAction, Error> {
    let client = ctx.get_ref().client.clone();
    let nodes: Api<Node> = Api::all(client);
    finalizer(&nodes, "k8s.haim.dev/linode-dns-finalizer", node, |event| {
        finalizer_reconcile(event, ctx)
    })
    .await?;

    Ok(ReconcilerAction { requeue_after: None })
}

/// The controller triggers this on reconcile errors
fn error_policy(error: &Error, _ctx: ControllerContext<ContextData>) -> ReconcilerAction {
    warn!(error = format!("{}", error).as_str(), "Reconcile failed");
    ReconcilerAction {
        requeue_after: Some(tokio::time::Duration::from_secs(30)),
    }
}

pub async fn run() -> Result<(), Error> {
    let node_domain = std::env::var("NODE_DOMAIN").context("NODE_DOMAIN environment variable is not defined")?;
    let linode_api_token =
        std::env::var("LINODE_API_TOKEN").context("LINODE_API_TOKEN environment variable is not defined")?;

    let client = kube::Client::try_default().await?;
    let nodes: Api<Node> = Api::all(client.clone());
    let lp = ListParams::default().fields("").timeout(290);

    let context_data = ContextData {
        client,
        node_domain,
        linode_api_token,
    };
    Controller::new(nodes, lp)
        .shutdown_on_signal()
        .run(reconcile, error_policy, ControllerContext::new(context_data))
        .for_each(|result| async move { trace!("Reconciled: {:?}", result) })
        .await;
    Ok(())
}
