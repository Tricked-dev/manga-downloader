pub use crate::descriptor::{
    CURRENT_PLUGIN_API_VERSION, SearchOptions, SourceCapability, SourceManifest,
    SourceManifestBuilder,
};
pub use crate::error::format_http_error;
pub use crate::fetch::{expect_success, parse_json};
pub use crate::request::{
    Header, HttpMethod, MediaRefParts, RequestParts, RequestPurpose, api_json, get_html, get_json,
    get_with_purpose, head_document, image_hotlink, media_direct, media_hotlink,
};
