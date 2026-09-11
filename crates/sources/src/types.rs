#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

#[derive(Clone, Debug)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestPurpose {
    Api,
    Document,
    Image,
    Asset,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceCapability {
    Search,
    MangaDetails,
    ChapterList,
    PageList,
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub categories: Vec<String>,
    pub default_category: Option<String>,
    pub supports_popular_sort: bool,
}

#[derive(Clone, Debug)]
pub struct SourceError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug)]
pub struct SourceMetadata {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub version: String,
    pub api_version: u32,
    pub build_metadata: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub capabilities: Vec<SourceCapability>,
    pub search_options: SearchOptions,
}

#[derive(Clone, Debug)]
pub struct Manga {
    pub id: String,
    pub title: String,
    pub cover: MediaRef,
    pub description: String,
    pub author: String,
    pub genres: Vec<String>,
    pub status: String,
    pub is_nsfw: bool,
    pub alt_titles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub number: f64,
    pub volume: Option<f64>,
    pub published_at: String,
    pub download_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub index: u32,
    pub image: MediaRef,
}

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub text: String,
    pub page: u32,
    pub category: Option<String>,
    pub popular: bool,
}

#[derive(Clone, Debug)]
pub struct SearchResults {
    pub items: Vec<Manga>,
    pub has_next_page: bool,
}

#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub url: String,
    pub method: HttpMethod,
    pub headers: Vec<HttpHeader>,
    pub body: Option<String>,
    pub purpose: RequestPurpose,
}

#[derive(Clone, Debug)]
pub struct FetchResponse {
    pub status: u16,
    pub headers: Vec<HttpHeader>,
    pub body: String,
    pub final_url: String,
}

#[derive(Clone, Debug)]
pub struct BrowserJsonCaptureRequest {
    pub url: String,
    pub init_script: String,
    pub done_expression: String,
    pub payloads_expression: String,
    pub timeout_ms: u32,
    pub poll_interval_ms: u32,
}

#[derive(Clone, Debug)]
pub struct BrowserJsonCaptureResponse {
    pub payloads: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MediaRef {
    pub url: String,
    pub request: Option<FetchRequest>,
}
