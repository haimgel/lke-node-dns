use crate::dns;
use crate::errors::Error;
use anyhow::{Context, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Node;
use kube::{
    api::{Api, ListParams},
    runtime::controller::{Context as ControllerContext, Controller, ReconcilerAction},
};
use std::net::IpAddr;
use tracing::{trace, warn};

/// Data we want access to in error/reconcile calls
struct ContextData {
    domain: String,
    linode_api_token: String,
}

/// Controller triggers this whenever any of the nodes have changed in any way
async fn reconcile(node: Node, ctx: ControllerContext<ContextData>) -> Result<ReconcilerAction, Error> {
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
    let external_ip: IpAddr = addresses
        .iter()
        .find(|address| address.type_ == "ExternalIP".to_string())
        .as_ref()
        .ok_or(Error::MissingObjectKey("status.addresses.ExternalIP"))?
        .address
        .as_str()
        .parse()
        .context("ExternalIP is not a valid IPv4 address")?;

    dns::update(
        ctx.get_ref().linode_api_token.as_str(),
        ctx.get_ref().domain.as_str(),
        host_name,
        external_ip,
    )
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
    let lp = ListParams::default().timeout(60);

    let context_data = ContextData {
        domain: node_domain,
        linode_api_token,
    };
    Controller::new(nodes, lp)
        .shutdown_on_signal()
        .run(reconcile, error_policy, ControllerContext::new(context_data))
        .for_each(|result| async move { trace!("Reconciled: {:?}", result) })
        .await;
    Ok(())
}
