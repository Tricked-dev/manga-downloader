use backend_plugin_host::media::{
    FetchRequestSpec, HttpHeaderSpec, MediaRefSpec, MediaTransformSpec, RequestPurposeSpec,
    decode_media_spec, encode_media_spec, media_spec_to_proxy_url, runtime_media_ref_to_spec,
};

#[test]
fn media_specs_round_trip_fetch_request_details() {
    let spec = MediaRefSpec {
        url: "https://cdn.example.test/page.avif".to_string(),
        request: Some(FetchRequestSpec {
            url: "https://origin.example.test/page".to_string(),
            method: "GET".to_string(),
            headers: vec![HttpHeaderSpec {
                name: "Referer".to_string(),
                value: "https://origin.example.test/title".to_string(),
            }],
            body: None,
            purpose: RequestPurposeSpec::Image,
        }),
        transform: None,
    };

    let encoded = encode_media_spec(&spec).expect("media spec should encode");
    let decoded = decode_media_spec(&encoded).expect("media spec should decode");

    assert_eq!(decoded, spec);
}

#[test]
fn proxy_url_embeds_an_encoded_media_spec() {
    let spec = MediaRefSpec {
        url: "https://cdn.example.test/page.avif".to_string(),
        request: None,
        transform: None,
    };

    let proxy_url = media_spec_to_proxy_url(&spec).expect("proxy url should encode");
    let encoded = proxy_url
        .strip_prefix("/v1/media/image?spec=")
        .expect("proxy url should expose spec query");

    assert_eq!(
        decode_media_spec(encoded).expect("query spec should decode"),
        spec
    );
}

#[test]
fn media_spec_validation_rejects_unsafe_urls_and_methods() {
    let file_url = MediaRefSpec {
        url: "file:///tmp/page.avif".to_string(),
        request: None,
        transform: None,
    };
    let post_request = MediaRefSpec {
        url: "https://cdn.example.test/page.avif".to_string(),
        request: Some(FetchRequestSpec {
            url: "https://origin.example.test/page".to_string(),
            method: "POST".to_string(),
            headers: Vec::new(),
            body: Some("payload".to_string()),
            purpose: RequestPurposeSpec::Image,
        }),
        transform: None,
    };

    assert!(encode_media_spec(&file_url).is_err());
    assert!(encode_media_spec(&post_request).is_err());
}

#[test]
fn runtime_media_ref_marker_sets_transform_and_strips_fragment() {
    let media = backend_plugin_host::runtime::manga::source::types::MediaRef {
        url: "https://cdn.example.test/page.webp#manga-server-transform=comix-descramble-5x5"
            .to_string(),
        request: None,
    };

    let spec = runtime_media_ref_to_spec(&media).expect("marker should decode");

    assert_eq!(spec.url, "https://cdn.example.test/page.webp");
    assert_eq!(spec.transform, Some(MediaTransformSpec::ComixDescramble5x5));
}

#[test]
fn runtime_media_ref_marker_accepts_dynamic_comix_tile_map() {
    let map = (0..25)
        .rev()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let media = backend_plugin_host::runtime::manga::source::types::MediaRef {
        url: format!(
            "https://cdn.example.test/page.webp#manga-server-transform=comix-descramble-5x5:{}",
            map.join(",")
        ),
        request: None,
    };

    let spec = runtime_media_ref_to_spec(&media).expect("marker should decode");

    assert_eq!(spec.url, "https://cdn.example.test/page.webp");
    assert_eq!(
        spec.transform,
        Some(MediaTransformSpec::ComixDescramble5x5Map(
            (0..25).rev().collect()
        ))
    );
}
