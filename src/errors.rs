use thiserror::Error;
use kube::runtime::finalizer::Error as FinalizerError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Missing object key: {0}")]
    MissingObjectKey(&'static str),
    #[error("Kubernetes API error: {0}")]
    KubeApiFailure(#[from] kube::error::Error),
    #[error("Missing environment variable {0}")]
    MissingEnvVar(#[from] std::env::VarError),
    #[error("Object has no name")]
    UnnamedObject,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<FinalizerError<Self>> for Error {
    fn from(err: FinalizerError<Self>) -> Self {
        match err {
            FinalizerError::ApplyFailed(err) => err,
            FinalizerError::CleanupFailed(err) => err,
            FinalizerError::AddFinalizer(err) => Self::KubeApiFailure(err),
            FinalizerError::RemoveFinalizer(err) => Self::KubeApiFailure(err),
            FinalizerError::UnnamedObject => Self::UnnamedObject,
        }
    }
}
