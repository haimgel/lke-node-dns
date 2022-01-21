use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Missing object key: {0}")]
    MissingObjectKey(&'static str),
    #[error("Kubernetes API error: {0}")]
    KubeApiFailure(#[from] kube::error::Error),
    #[error("Missing environment variable {0}")]
    MissingEnvVar(#[from] std::env::VarError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
