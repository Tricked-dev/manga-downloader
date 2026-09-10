use url::form_urlencoded::Serializer;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::types;

const MAX_MEDIA_SPEC_BYTES: usize = 16 * 1024;
const COMIX_DESCRAMBLE_FRAGMENT: &str = "manga-server-transform=comix-descramble-5x5";
const COMIX_DESCRAMBLE_MAP_PREFIX: &str = "manga-server-transform=comix-descramble-5x5:";

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "kebab-case")]
pub enum RequestPurposeSpec {
    Api,
    Document,
    Image,
    Asset,
    Custom,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    PartialEq,
    Eq,
)]
pub struct HttpHeaderSpec {
    pub name: String,
    pub value: String,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    PartialEq,
    Eq,
)]
pub struct FetchRequestSpec {
    pub url: String,
    #[serde(default = "default_http_method")]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<HttpHeaderSpec>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default = "default_request_purpose")]
    pub purpose: RequestPurposeSpec,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    PartialEq,
    Eq,
)]
pub struct MediaRefSpec {
    pub url: String,
    #[serde(default)]
    pub request: Option<FetchRequestSpec>,
    #[serde(default)]
    pub transform: Option<MediaTransformSpec>,
}

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    PartialEq,
    Eq,
)]
pub enum MediaTransformSpec {
    #[serde(rename = "comix-descramble-5x5")]
    ComixDescramble5x5,
    #[serde(rename = "comix-descramble-5x5-map")]
    ComixDescramble5x5Map(Vec<usize>),
}

/// Encodes a media reference spec into a URL-safe proxy token.
pub fn encode_media_spec(spec: &MediaRefSpec) -> anyhow::Result<String> {
    validate_media_spec(spec)?;
    let json = serde_json::to_vec(spec)?;
    if json.len() > MAX_MEDIA_SPEC_BYTES {
        anyhow::bail!("Encoded media spec exceeds size limit");
    }
    Ok(URL_SAFE_NO_PAD.encode(json))
}

/// Decodes and validates a URL-safe media proxy token.
pub fn decode_media_spec(encoded: &str) -> anyhow::Result<MediaRefSpec> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|err| anyhow::anyhow!("Invalid media spec encoding: {err}"))?;
    if bytes.len() > MAX_MEDIA_SPEC_BYTES {
        anyhow::bail!("Encoded media spec exceeds size limit");
    }
    let spec: MediaRefSpec = serde_json::from_slice(&bytes)
        .map_err(|err| anyhow::anyhow!("Invalid media spec payload: {err}"))?;
    validate_media_spec(&spec)?;
    Ok(spec)
}

/// Builds the application media proxy URL for a media spec.
pub fn media_spec_to_proxy_url(spec: &MediaRefSpec) -> anyhow::Result<String> {
    Ok(format!(
        "/v1/media/image?{}",
        Serializer::new(String::new())
            .append_pair("spec", &encode_media_spec(spec)?)
            .finish()
    ))
}

/// Converts a plugin runtime media reference into a validated host media spec.
pub fn media_ref_to_spec(media: &types::MediaRef) -> anyhow::Result<MediaRefSpec> {
    let mut spec = MediaRefSpec {
        url: media.url.clone(),
        request: media.request.as_ref().map(request_to_spec),
        transform: None,
    };
    extract_transform_marker(&mut spec)?;
    validate_media_spec(&spec)?;
    Ok(spec)
}

#[must_use]
/// Converts a plugin runtime fetch request into the serializable host request spec.
pub fn request_to_spec(request: &types::FetchRequest) -> FetchRequestSpec {
    FetchRequestSpec {
        url: request.url.clone(),
        method: http_method_name(request.method),
        headers: request
            .headers
            .iter()
            .map(|header| HttpHeaderSpec {
                name: header.name.clone(),
                value: header.value.clone(),
            })
            .collect(),
        body: request.body.clone(),
        purpose: request_purpose_to_spec(request.purpose),
    }
}

fn validate_media_spec(spec: &MediaRefSpec) -> anyhow::Result<()> {
    validate_url(&spec.url)?;
    if let Some(transform) = &spec.transform {
        validate_transform(transform)?;
    }
    if let Some(request) = &spec.request {
        validate_url(&request.url)?;
        let method = request.method.trim().to_ascii_uppercase();
        if method != "GET" {
            anyhow::bail!("Only GET media requests are supported");
        }
    }
    Ok(())
}

fn validate_transform(transform: &MediaTransformSpec) -> anyhow::Result<()> {
    match transform {
        MediaTransformSpec::ComixDescramble5x5 => Ok(()),
        MediaTransformSpec::ComixDescramble5x5Map(map) => validate_comix_tile_map(map),
    }
}

fn extract_transform_marker(spec: &mut MediaRefSpec) -> anyhow::Result<()> {
    let Some(transform) = extract_url_transform_marker(&mut spec.url)? else {
        return Ok(());
    };
    spec.transform = Some(transform);
    if let Some(request) = &mut spec.request {
        let request_transform = extract_url_transform_marker(&mut request.url)?;
        if request_transform.is_some() && request_transform != spec.transform {
            anyhow::bail!("Conflicting media transform markers");
        }
    }
    Ok(())
}

fn extract_url_transform_marker(url: &mut String) -> anyhow::Result<Option<MediaTransformSpec>> {
    if url.is_empty() {
        return Ok(None);
    }

    let mut parsed =
        url::Url::parse(url).map_err(|err| anyhow::anyhow!("Invalid URL '{url}': {err}"))?;
    let transform = match parsed.fragment() {
        Some(COMIX_DESCRAMBLE_FRAGMENT) => Some(MediaTransformSpec::ComixDescramble5x5),
        Some(fragment) if fragment.starts_with(COMIX_DESCRAMBLE_MAP_PREFIX) => Some(
            MediaTransformSpec::ComixDescramble5x5Map(parse_comix_tile_map(
                fragment
                    .strip_prefix(COMIX_DESCRAMBLE_MAP_PREFIX)
                    .unwrap_or_default(),
            )?),
        ),
        Some(fragment) if fragment.starts_with("manga-server-transform=") => {
            anyhow::bail!("Unsupported media transform marker '{fragment}'")
        }
        _ => None,
    };
    if transform.is_some() {
        parsed.set_fragment(None);
        *url = parsed.to_string();
    }
    Ok(transform)
}

fn parse_comix_tile_map(value: &str) -> anyhow::Result<Vec<usize>> {
    let map = value
        .split(',')
        .map(|part| {
            part.parse::<usize>()
                .map_err(|err| anyhow::anyhow!("Invalid Comix tile map value '{part}': {err}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    validate_comix_tile_map(&map)?;
    Ok(map)
}

fn validate_comix_tile_map(map: &[usize]) -> anyhow::Result<()> {
    if map.len() != 25 {
        anyhow::bail!("Comix tile map must contain 25 entries");
    }

    let mut seen = [false; 25];
    for &value in map {
        if value >= 25 {
            anyhow::bail!("Comix tile map value {value} is out of range");
        }
        if seen[value] {
            anyhow::bail!("Comix tile map value {value} appears more than once");
        }
        seen[value] = true;
    }
    Ok(())
}

fn validate_url(url: &str) -> anyhow::Result<()> {
    if url.is_empty() {
        return Ok(());
    }
    let parsed =
        url::Url::parse(url).map_err(|err| anyhow::anyhow!("Invalid URL '{url}': {err}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => anyhow::bail!("Unsupported URL scheme '{scheme}'"),
    }
}

fn http_method_name(method: types::HttpMethod) -> String {
    match method {
        types::HttpMethod::Get => "GET",
        types::HttpMethod::Post => "POST",
        types::HttpMethod::Put => "PUT",
        types::HttpMethod::Patch => "PATCH",
        types::HttpMethod::Delete => "DELETE",
        types::HttpMethod::Head => "HEAD",
        types::HttpMethod::Options => "OPTIONS",
    }
    .to_string()
}

fn request_purpose_to_spec(purpose: types::RequestPurpose) -> RequestPurposeSpec {
    match purpose {
        types::RequestPurpose::Api => RequestPurposeSpec::Api,
        types::RequestPurpose::Document => RequestPurposeSpec::Document,
        types::RequestPurpose::Image => RequestPurposeSpec::Image,
        types::RequestPurpose::Asset => RequestPurposeSpec::Asset,
        types::RequestPurpose::Custom => RequestPurposeSpec::Custom,
    }
}

fn default_http_method() -> String {
    "GET".to_string()
}

fn default_request_purpose() -> RequestPurposeSpec {
    RequestPurposeSpec::Image
}
