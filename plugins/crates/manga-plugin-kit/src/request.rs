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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestPurpose {
    Api,
    Document,
    Image,
    Asset,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestParts {
    pub url: String,
    pub method: HttpMethod,
    pub headers: Vec<Header>,
    pub body: Option<String>,
    pub purpose: RequestPurpose,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaRefParts {
    pub url: String,
    pub request: Option<RequestParts>,
}

#[must_use]
pub fn get_json(url: &str) -> RequestParts {
    api_json(url)
}

#[must_use]
pub fn get_html(url: &str) -> RequestParts {
    get_with_purpose(url, RequestPurpose::Document)
}

#[must_use]
pub fn api_json(url: &str) -> RequestParts {
    get_with_purpose(url, RequestPurpose::Api)
}

#[must_use]
pub fn head_document(url: &str) -> RequestParts {
    RequestParts {
        url: url.to_string(),
        method: HttpMethod::Head,
        headers: Vec::new(),
        body: None,
        purpose: RequestPurpose::Document,
    }
}

#[must_use]
pub fn image_hotlink(url: &str, referer_root: &str) -> RequestParts {
    let mut request = get_with_purpose(url, RequestPurpose::Image);
    request.headers.push(Header {
        name: "Referer".to_string(),
        value: referer_root.to_string(),
    });
    request
}

#[must_use]
pub fn get_with_purpose(url: &str, purpose: RequestPurpose) -> RequestParts {
    RequestParts {
        url: url.to_string(),
        method: HttpMethod::Get,
        headers: Vec::new(),
        body: None,
        purpose,
    }
}

#[must_use]
pub fn media_direct(url: &str) -> MediaRefParts {
    MediaRefParts {
        url: url.to_string(),
        request: None,
    }
}

#[must_use]
pub fn media_hotlink(url: &str, referer_root: &str) -> MediaRefParts {
    MediaRefParts {
        url: url.to_string(),
        request: Some(image_hotlink(url, referer_root)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_builders_mark_purpose_and_method() {
        assert_eq!(
            get_json("https://example.test/api").purpose,
            RequestPurpose::Api
        );
        assert_eq!(
            get_html("https://example.test").purpose,
            RequestPurpose::Document
        );

        let head = head_document("https://example.test");
        assert_eq!(head.method, HttpMethod::Head);
        assert_eq!(head.purpose, RequestPurpose::Document);
        assert!(head.headers.is_empty());
        assert!(head.body.is_none());
    }

    #[test]
    fn hotlink_builders_attach_referer_request_metadata() {
        let request = image_hotlink("https://cdn.example.test/page.jpg", "https://example.test");
        assert_eq!(request.method, HttpMethod::Get);
        assert_eq!(request.purpose, RequestPurpose::Image);
        assert_eq!(
            request.headers,
            vec![Header {
                name: "Referer".to_string(),
                value: "https://example.test".to_string()
            }]
        );

        let media = media_hotlink("https://cdn.example.test/page.jpg", "https://example.test");
        assert_eq!(media.url, "https://cdn.example.test/page.jpg");
        assert_eq!(media.request, Some(request));
    }

    #[test]
    fn direct_media_does_not_require_fetch_metadata() {
        assert_eq!(
            media_direct("https://cdn.example.test/page.jpg"),
            MediaRefParts {
                url: "https://cdn.example.test/page.jpg".to_string(),
                request: None
            }
        );
    }
}
