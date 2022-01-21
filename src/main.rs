use node_dns::controller;
use node_dns::logging;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    logging::setup();
    std::process::exit(controller::run().await.map_or_else(
        |error| {
            error!(error = format!("{}", error).as_str(), "Fatal error, terminating");
            1
        },
        |_| {
            info!("Controller terminated");
            0
        },
    ));
}
