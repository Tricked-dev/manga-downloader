use std::fmt;
use std::sync::OnceLock;

static RUSTLS_PROVIDER_INIT: OnceLock<Result<(), GraviolaProviderError>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraviolaProviderError;

impl fmt::Display for GraviolaProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("rustls crypto provider was initialized before Graviola could be installed")
    }
}

impl std::error::Error for GraviolaProviderError {}

/// Installs the Graviola rustls crypto provider for the current process.
///
/// This must run before any other rustls provider is installed.
pub fn ensure_graviola_rustls_provider() -> Result<(), GraviolaProviderError> {
    RUSTLS_PROVIDER_INIT
        .get_or_init(|| {
            // reqwest's rustls-no-provider mode requires a process-wide provider before any client
            // is built. Install Graviola once so every reqwest client in this process shares it.
            rustls_graviola::default_provider()
                .install_default()
                .map_err(|_| GraviolaProviderError)
        })
        .to_owned()
}
