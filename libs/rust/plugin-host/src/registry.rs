use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use backend_tls::GraviolaProviderError;
use reqwest::StatusCode;
use semver::Version;
use sha2::{Digest, Sha256};
use url::Url;

use crate::fetch::{DEFAULT_USER_AGENT, ensure_graviola_rustls_provider};

pub const SOURCE_PLUGIN_REGISTRY_MANIFEST_VERSION: u32 = 1;
const REGISTRY_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Manifest shape for a remote source-plugin registry.
///
/// Every artifact must be pinned by SHA-256, with room for signature metadata.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourcePluginRegistryManifest {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub plugins: BTreeMap<String, SourcePluginRegistryPlugin>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourcePluginRegistryPlugin {
    pub description: Option<String>,
    #[serde(default)]
    pub versions: Vec<SourcePluginRegistryVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourcePluginRegistryVersion {
    pub version: String,
    pub plugin_api_version: u32,
    pub artifact: SourcePluginRegistryArtifact,
    #[serde(default)]
    pub deprecated: bool,
    pub deprecation_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourcePluginRegistryArtifact {
    pub url: String,
    pub sha256: String,
    pub signature: Option<SourcePluginRegistrySignature>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourcePluginRegistrySignature {
    pub key_id: String,
    pub algorithm: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourcePluginRegistry {
    manifest: SourcePluginRegistryManifest,
}

impl SourcePluginRegistry {
    pub fn from_manifest(
        manifest: SourcePluginRegistryManifest,
    ) -> Result<Self, SourcePluginRegistryManifestError> {
        validate_registry_manifest_security(&manifest)?;
        Ok(Self { manifest })
    }

    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, SourcePluginRegistryLoadError> {
        let manifest =
            serde_json::from_slice(bytes).map_err(SourcePluginRegistryLoadError::Parse)?;
        Self::from_manifest(manifest).map_err(SourcePluginRegistryLoadError::InvalidManifest)
    }

    pub fn from_json_str(value: &str) -> Result<Self, SourcePluginRegistryLoadError> {
        Self::from_json_slice(value.as_bytes())
    }

    #[must_use]
    pub fn manifest(&self) -> &SourcePluginRegistryManifest {
        &self.manifest
    }

    #[must_use]
    pub fn plugins(&self) -> &BTreeMap<String, SourcePluginRegistryPlugin> {
        &self.manifest.plugins
    }

    #[must_use]
    pub fn plugin(&self, plugin_id: &str) -> Option<&SourcePluginRegistryPlugin> {
        self.manifest.plugins.get(plugin_id)
    }

    pub fn versions(
        &self,
        plugin_id: &str,
    ) -> Result<&[SourcePluginRegistryVersion], SourcePluginRegistryLookupError> {
        self.plugin(plugin_id)
            .map(|plugin| plugin.versions.as_slice())
            .ok_or_else(|| SourcePluginRegistryLookupError::PluginNotFound {
                plugin_id: plugin_id.to_string(),
            })
    }

    pub fn resolve_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> Result<&SourcePluginRegistryVersion, SourcePluginRegistryLookupError> {
        self.versions(plugin_id)?
            .iter()
            .find(|candidate| candidate.version == version)
            .ok_or_else(|| SourcePluginRegistryLookupError::VersionNotFound {
                plugin_id: plugin_id.to_string(),
                version: version.to_string(),
            })
    }

    pub fn compatible_versions(
        &self,
        plugin_id: &str,
        plugin_api_version: u32,
    ) -> Result<Vec<&SourcePluginRegistryVersion>, SourcePluginRegistryLookupError> {
        Ok(self
            .versions(plugin_id)?
            .iter()
            .filter(|version| {
                version.plugin_api_version == plugin_api_version && !version.deprecated
            })
            .collect())
    }

    pub fn latest_compatible_version(
        &self,
        plugin_id: &str,
        plugin_api_version: u32,
    ) -> Result<&SourcePluginRegistryVersion, SourcePluginRegistryLookupError> {
        self.compatible_versions(plugin_id, plugin_api_version)?
            .into_iter()
            .max_by(|left, right| compare_registry_versions(&left.version, &right.version))
            .ok_or_else(|| SourcePluginRegistryLookupError::NoCompatibleVersion {
                plugin_id: plugin_id.to_string(),
                plugin_api_version,
            })
    }
}

#[derive(Clone)]
pub struct SourcePluginRegistryClient {
    http: reqwest::Client,
}

impl SourcePluginRegistryClient {
    pub fn new() -> Result<Self, SourcePluginRegistryClientError> {
        ensure_graviola_rustls_provider().map_err(SourcePluginRegistryClientError::TlsProvider)?;
        let http = reqwest::Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .timeout(REGISTRY_HTTP_TIMEOUT)
            .zstd(true)
            .build()
            .map_err(SourcePluginRegistryClientError::HttpClient)?;

        Ok(Self { http })
    }

    #[must_use]
    pub fn with_http_client(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub async fn fetch_manifest(
        &self,
        url: &str,
    ) -> Result<SourcePluginRegistry, SourcePluginRegistryFetchError> {
        let bytes = self.fetch_bytes(url, "manifest").await?;
        SourcePluginRegistry::from_json_slice(&bytes).map_err(SourcePluginRegistryFetchError::Load)
    }

    pub async fn fetch_artifact(
        &self,
        plugin_id: &str,
        version: &SourcePluginRegistryVersion,
    ) -> Result<Vec<u8>, SourcePluginRegistryFetchError> {
        let bytes = self.fetch_bytes(&version.artifact.url, "artifact").await?;
        if !verify_registry_artifact_sha256(&version.artifact, &bytes) {
            return Err(SourcePluginRegistryFetchError::ArtifactDigestMismatch {
                plugin_id: plugin_id.to_string(),
                version: version.version.clone(),
            });
        }
        Ok(bytes)
    }

    async fn fetch_bytes(
        &self,
        url: &str,
        kind: &'static str,
    ) -> Result<Vec<u8>, SourcePluginRegistryFetchError> {
        let response = self.http.get(url).send().await.map_err(|source| {
            SourcePluginRegistryFetchError::Request {
                kind,
                url: url.to_string(),
                source,
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(SourcePluginRegistryFetchError::HttpStatus {
                kind,
                url: url.to_string(),
                status,
            });
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|source| SourcePluginRegistryFetchError::Body {
                kind,
                url: url.to_string(),
                source,
            })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourcePluginRegistryClientError {
    #[error("failed to initialize TLS provider for source plugin registry client")]
    TlsProvider(#[source] GraviolaProviderError),
    #[error("failed to build source plugin registry HTTP client")]
    HttpClient(#[source] reqwest::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SourcePluginRegistryLoadError {
    #[error("failed to parse source plugin registry manifest")]
    Parse(#[source] serde_json::Error),
    #[error(transparent)]
    InvalidManifest(#[from] SourcePluginRegistryManifestError),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SourcePluginRegistryLookupError {
    #[error("plugin '{plugin_id}' was not found in the source plugin registry")]
    PluginNotFound { plugin_id: String },
    #[error("plugin '{plugin_id}' version '{version}' was not found in the source plugin registry")]
    VersionNotFound { plugin_id: String, version: String },
    #[error(
        "plugin '{plugin_id}' has no non-deprecated version compatible with plugin API {plugin_api_version}"
    )]
    NoCompatibleVersion {
        plugin_id: String,
        plugin_api_version: u32,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum SourcePluginRegistryFetchError {
    #[error("failed to request source plugin registry {kind} '{url}': {source}")]
    Request {
        kind: &'static str,
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to read source plugin registry {kind} '{url}': {source}")]
    Body {
        kind: &'static str,
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("source plugin registry {kind} '{url}' returned HTTP {status}")]
    HttpStatus {
        kind: &'static str,
        url: String,
        status: StatusCode,
    },
    #[error(transparent)]
    Load(#[from] SourcePluginRegistryLoadError),
    #[error(transparent)]
    Lookup(#[from] SourcePluginRegistryLookupError),
    #[error(
        "downloaded artifact for plugin '{plugin_id}' version '{version}' did not match SHA-256"
    )]
    ArtifactDigestMismatch { plugin_id: String, version: String },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SourcePluginRegistryManifestError {
    #[error("unsupported source plugin registry manifest version {version}")]
    UnsupportedManifestVersion { version: u32 },
    #[error("source plugin registry manifest name is required")]
    EmptyRegistryName,
    #[error("source plugin registry contains an empty plugin id")]
    EmptyPluginId,
    #[error("plugin '{plugin_id}' contains an empty version")]
    EmptyPluginVersion { plugin_id: String },
    #[error("plugin '{plugin_id}' version '{version}' is not valid SemVer")]
    InvalidPluginVersion { plugin_id: String, version: String },
    #[error("plugin '{plugin_id}' contains duplicate version '{version}'")]
    DuplicatePluginVersion { plugin_id: String, version: String },
    #[error("plugin '{plugin_id}' version '{version}' has an empty artifact URL")]
    EmptyArtifactUrl { plugin_id: String, version: String },
    #[error("plugin '{plugin_id}' version '{version}' has an invalid artifact URL")]
    InvalidArtifactUrl { plugin_id: String, version: String },
    #[error("plugin '{plugin_id}' version '{version}' has an invalid SHA-256 digest")]
    InvalidSha256Digest { plugin_id: String, version: String },
    #[error(
        "plugin '{plugin_id}' version '{version}' has empty signature metadata field '{field}'"
    )]
    EmptySignatureMetadata {
        plugin_id: String,
        version: String,
        field: &'static str,
    },
}

pub fn validate_registry_manifest_security(
    manifest: &SourcePluginRegistryManifest,
) -> Result<(), SourcePluginRegistryManifestError> {
    if manifest.version != SOURCE_PLUGIN_REGISTRY_MANIFEST_VERSION {
        return Err(
            SourcePluginRegistryManifestError::UnsupportedManifestVersion {
                version: manifest.version,
            },
        );
    }
    if manifest.name.trim().is_empty() {
        return Err(SourcePluginRegistryManifestError::EmptyRegistryName);
    }

    for (plugin_id, plugin) in &manifest.plugins {
        if plugin_id.trim().is_empty() {
            return Err(SourcePluginRegistryManifestError::EmptyPluginId);
        }

        let mut seen_versions = BTreeSet::new();
        for version in &plugin.versions {
            let version_id = version.version.trim();
            if version_id.is_empty() {
                return Err(SourcePluginRegistryManifestError::EmptyPluginVersion {
                    plugin_id: plugin_id.clone(),
                });
            }
            if Version::parse(version_id).is_err() {
                return Err(SourcePluginRegistryManifestError::InvalidPluginVersion {
                    plugin_id: plugin_id.clone(),
                    version: version_id.to_string(),
                });
            }
            if !seen_versions.insert(version_id) {
                return Err(SourcePluginRegistryManifestError::DuplicatePluginVersion {
                    plugin_id: plugin_id.clone(),
                    version: version_id.to_string(),
                });
            }

            let artifact_url = version.artifact.url.trim();
            if artifact_url.is_empty() {
                return Err(SourcePluginRegistryManifestError::EmptyArtifactUrl {
                    plugin_id: plugin_id.clone(),
                    version: version.version.clone(),
                });
            }
            if !is_remote_url(artifact_url) {
                return Err(SourcePluginRegistryManifestError::InvalidArtifactUrl {
                    plugin_id: plugin_id.clone(),
                    version: version.version.clone(),
                });
            }
            if !is_sha256_hex_digest(&version.artifact.sha256) {
                return Err(SourcePluginRegistryManifestError::InvalidSha256Digest {
                    plugin_id: plugin_id.clone(),
                    version: version.version.clone(),
                });
            }
            if let Some(signature) = &version.artifact.signature {
                validate_signature_metadata(plugin_id, &version.version, signature)?;
            }
        }
    }

    Ok(())
}

#[must_use]
pub fn compute_registry_artifact_sha256(bytes: &[u8]) -> String {
    hex_encode(&Sha256::digest(bytes))
}

pub fn verify_registry_artifact_sha256(
    artifact: &SourcePluginRegistryArtifact,
    bytes: &[u8],
) -> bool {
    compute_registry_artifact_sha256(bytes).eq_ignore_ascii_case(&artifact.sha256)
}

fn is_sha256_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_remote_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https") && url.has_host()
}

fn validate_signature_metadata(
    plugin_id: &str,
    version: &str,
    signature: &SourcePluginRegistrySignature,
) -> Result<(), SourcePluginRegistryManifestError> {
    for (field, value) in [
        ("key_id", signature.key_id.as_str()),
        ("algorithm", signature.algorithm.as_str()),
        ("value", signature.value.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(SourcePluginRegistryManifestError::EmptySignatureMetadata {
                plugin_id: plugin_id.to_string(),
                version: version.to_string(),
                field,
            });
        }
    }

    Ok(())
}

fn compare_registry_versions(left: &str, right: &str) -> Ordering {
    match (Version::parse(left), Version::parse(right)) {
        (Ok(left), Ok(right)) => left.cmp_precedence(&right),
        _ => left.cmp(right),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> SourcePluginRegistryManifest {
        let mut manifest = SourcePluginRegistryManifest {
            version: SOURCE_PLUGIN_REGISTRY_MANIFEST_VERSION,
            name: "demo".to_string(),
            plugins: BTreeMap::new(),
        };
        manifest.plugins.insert(
            "source".to_string(),
            SourcePluginRegistryPlugin {
                description: None,
                versions: vec![SourcePluginRegistryVersion {
                    version: "1.0.0".to_string(),
                    plugin_api_version: crate::PLUGIN_API_VERSION,
                    artifact: SourcePluginRegistryArtifact {
                        url: "https://example.com/source.wasm".to_string(),
                        sha256: compute_registry_artifact_sha256(b"wasm").to_string(),
                        signature: None,
                    },
                    deprecated: false,
                    deprecation_reason: None,
                }],
            },
        );
        manifest
    }

    #[test]
    fn registry_manifest_security_requires_sha256_pinned_artifacts() {
        let mut manifest = sample_manifest();

        validate_registry_manifest_security(&manifest).unwrap();

        manifest
            .plugins
            .get_mut("source")
            .unwrap()
            .versions
            .first_mut()
            .unwrap()
            .artifact
            .sha256 = "not-a-digest".to_string();

        assert!(matches!(
            validate_registry_manifest_security(&manifest),
            Err(SourcePluginRegistryManifestError::InvalidSha256Digest { .. })
        ));
    }

    #[test]
    fn registry_manifest_security_rejects_invalid_artifact_urls() {
        let mut manifest = sample_manifest();
        manifest
            .plugins
            .get_mut("source")
            .unwrap()
            .versions
            .first_mut()
            .unwrap()
            .artifact
            .url = "file:///tmp/source.wasm".to_string();

        assert!(matches!(
            validate_registry_manifest_security(&manifest),
            Err(SourcePluginRegistryManifestError::InvalidArtifactUrl { .. })
        ));
    }

    #[test]
    fn registry_manifest_security_rejects_duplicate_versions() {
        let mut manifest = sample_manifest();
        let duplicate = manifest
            .plugins
            .get("source")
            .unwrap()
            .versions
            .first()
            .unwrap()
            .clone();
        manifest
            .plugins
            .get_mut("source")
            .unwrap()
            .versions
            .push(duplicate);

        assert!(matches!(
            validate_registry_manifest_security(&manifest),
            Err(SourcePluginRegistryManifestError::DuplicatePluginVersion { .. })
        ));
    }

    #[test]
    fn registry_manifest_security_rejects_invalid_semver_versions() {
        let mut manifest = sample_manifest();
        manifest
            .plugins
            .get_mut("source")
            .unwrap()
            .versions
            .first_mut()
            .unwrap()
            .version = "1".to_string();

        assert!(matches!(
            validate_registry_manifest_security(&manifest),
            Err(SourcePluginRegistryManifestError::InvalidPluginVersion { .. })
        ));
    }

    #[test]
    fn registry_loads_from_json_and_resolves_versions() {
        let registry = SourcePluginRegistry::from_json_str(
            &serde_json::to_string(&sample_manifest()).unwrap(),
        )
        .unwrap();

        let version = registry
            .resolve_version("source", "1.0.0")
            .expect("sample version should resolve");

        assert_eq!(version.artifact.url, "https://example.com/source.wasm");
    }

    #[test]
    fn registry_selects_latest_compatible_non_deprecated_version() {
        let mut manifest = sample_manifest();
        manifest
            .plugins
            .get_mut("source")
            .unwrap()
            .versions
            .extend([
                SourcePluginRegistryVersion {
                    version: "1.10.0".to_string(),
                    plugin_api_version: crate::PLUGIN_API_VERSION,
                    artifact: SourcePluginRegistryArtifact {
                        url: "https://example.com/source-1.10.0.wasm".to_string(),
                        sha256: compute_registry_artifact_sha256(b"wasm-1.10.0"),
                        signature: None,
                    },
                    deprecated: true,
                    deprecation_reason: Some("bad release".to_string()),
                },
                SourcePluginRegistryVersion {
                    version: "1.2.0".to_string(),
                    plugin_api_version: crate::PLUGIN_API_VERSION,
                    artifact: SourcePluginRegistryArtifact {
                        url: "https://example.com/source-1.2.0.wasm".to_string(),
                        sha256: compute_registry_artifact_sha256(b"wasm-1.2.0"),
                        signature: None,
                    },
                    deprecated: false,
                    deprecation_reason: None,
                },
            ]);
        let registry = SourcePluginRegistry::from_manifest(manifest).unwrap();

        let version = registry
            .latest_compatible_version("source", crate::PLUGIN_API_VERSION)
            .unwrap();

        assert_eq!(version.version, "1.2.0");
    }

    #[test]
    fn registry_verifies_artifact_digest() {
        let version = sample_manifest()
            .plugins
            .remove("source")
            .unwrap()
            .versions
            .remove(0);

        assert!(verify_registry_artifact_sha256(&version.artifact, b"wasm"));
        assert!(!verify_registry_artifact_sha256(
            &version.artifact,
            b"tampered"
        ));
    }

    #[test]
    fn registry_version_comparison_handles_semver_like_versions() {
        assert_eq!(
            compare_registry_versions("1.10.0", "1.2.0"),
            Ordering::Greater
        );
        assert_eq!(
            compare_registry_versions("1.0.0-rc.1", "1.0.0"),
            Ordering::Less
        );
        assert_eq!(
            compare_registry_versions("1.0.0+build.1", "1.0.0"),
            Ordering::Equal
        );
    }
}
