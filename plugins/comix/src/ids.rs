use crate::http::get_json_body;
use crate::parse::parse_manga_details_response;
use crate::{API_URL, BASE_URL};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use url::Url;

#[derive(Clone, Copy)]
enum ByteMutation {
    Sub47,
    Sub70,
    Sub147,
    Sub181,
    Sub226,
    RotateLeft1,
    RotateLeft5,
    RotateRight2,
    RotateRight3,
    Xor28,
    Xor37,
    Xor208,
}

#[derive(Clone, Copy)]
struct HashRound {
    mutation_key: &'static str,
    prefix: &'static str,
    prefix_len: usize,
    mutations: [ByteMutation; 10],
}

enum HashStep {
    Cipher(&'static str),
    Mutate(HashRound),
}

const HASH_STEPS: [HashStep; 10] = [
    HashStep::Cipher("EO8fB2AQIKXZ5A/qaoglOT88IrBPN9r8lRNmm+KEUzI="),
    HashStep::Mutate(HashRound {
        mutation_key: "hGD3WVRsARKGT1Sx9JF9+E3IHOGwOIpssqTtWArFoO4=",
        prefix: "jUctkam5GFGxUA==",
        prefix_len: 10,
        mutations: [
            ByteMutation::Xor208,
            ByteMutation::Sub70,
            ByteMutation::Xor37,
            ByteMutation::Sub226,
            ByteMutation::Sub181,
            ByteMutation::RotateLeft5,
            ByteMutation::Xor28,
            ByteMutation::RotateRight2,
            ByteMutation::Sub181,
            ByteMutation::Xor208,
        ],
    }),
    HashStep::Cipher("Ln8y/7k8kWdMHrULDE9x/aalNWbCK+/vC/8gAihXlAQ="),
    HashStep::Mutate(HashRound {
        mutation_key: "iLirVhvDSgvOgxahVeFYx70TnBt0gOtsaQRjPlj5EH8=",
        prefix: "bcbQp+o6",
        prefix_len: 6,
        mutations: [
            ByteMutation::Sub181,
            ByteMutation::Sub47,
            ByteMutation::Xor28,
            ByteMutation::Sub70,
            ByteMutation::Xor28,
            ByteMutation::Sub181,
            ByteMutation::Xor28,
            ByteMutation::Xor208,
            ByteMutation::RotateLeft5,
            ByteMutation::RotateRight3,
        ],
    }),
    HashStep::Cipher("IkY+JZt8Zh4iUvPLDGGztNncx0f4i+VyCfk8b5vY4P0="),
    HashStep::Mutate(HashRound {
        mutation_key: "eICYaqic3pAk1ThfI33wRMxn8IXxyy8DXHfWOx5EGHY=",
        prefix: "Gi+iYUq9",
        prefix_len: 6,
        mutations: [
            ByteMutation::RotateRight3,
            ByteMutation::Xor37,
            ByteMutation::RotateLeft1,
            ByteMutation::RotateRight3,
            ByteMutation::RotateLeft1,
            ByteMutation::Sub147,
            ByteMutation::RotateLeft1,
            ByteMutation::Xor208,
            ByteMutation::Sub147,
            ByteMutation::Sub226,
        ],
    }),
    HashStep::Cipher("k80C/WNNoQeupQlmMdyc60+3WQPiJYY+PRy4Ca3jew8="),
    HashStep::Mutate(HashRound {
        mutation_key: "v/CWoFcLje+WM+9vRvWkkBtvvMTtYOAVejBf3+b+cJc=",
        prefix: "eBRPAsbPDw==",
        prefix_len: 7,
        mutations: [
            ByteMutation::RotateLeft5,
            ByteMutation::Sub181,
            ByteMutation::RotateLeft5,
            ByteMutation::Xor28,
            ByteMutation::Sub147,
            ByteMutation::RotateRight3,
            ByteMutation::Sub70,
            ByteMutation::Sub226,
            ByteMutation::Sub70,
            ByteMutation::Sub147,
        ],
    }),
    HashStep::Cipher("aUvDZX3P3oZ53+JPe68doZCPPyTlX2I8LNmQU9dew7U="),
    HashStep::Mutate(HashRound {
        mutation_key: "vCN7sFSIzrrs1lZ7cC3bWQldvHXNWPocVLAvgwgUs1w=",
        prefix: "YUCisHAu3f3E",
        prefix_len: 9,
        mutations: [
            ByteMutation::Xor37,
            ByteMutation::Xor208,
            ByteMutation::Sub226,
            ByteMutation::Sub147,
            ByteMutation::Sub226,
            ByteMutation::RotateLeft5,
            ByteMutation::Sub70,
            ByteMutation::Xor28,
            ByteMutation::Xor37,
            ByteMutation::Xor28,
        ],
    }),
];

fn is_numeric_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())
}

pub fn candidate_manga_ids(manga_id: &str, prefer_hash: bool) -> Vec<String> {
    let mut candidates = manga_id
        .split('|')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if prefer_hash {
        candidates.sort_by_key(|id| i32::from(is_numeric_id(id)));
    }

    let numeric_ids = candidates
        .iter()
        .filter(|id| is_numeric_id(id))
        .cloned()
        .collect::<Vec<_>>();

    for numeric_id in numeric_ids {
        if let Some(hash_id) = resolve_hash_id(&numeric_id)
            && !candidates.iter().any(|id| id == &hash_id)
        {
            if prefer_hash {
                candidates.insert(0, hash_id);
            } else {
                candidates.push(hash_id);
            }
        }
    }

    candidates
}

pub fn build_search_urls(
    query: &str,
    category: Option<&str>,
    popular: bool,
    page: u32,
) -> Vec<String> {
    let trimmed_query = query.trim();
    let trimmed_category = category.map(str::trim).filter(|value| !value.is_empty());
    let browse_order = browse_order(trimmed_category, popular);

    let genre = trimmed_category.filter(|value| {
        !matches!(
            value.to_ascii_lowercase().as_str(),
            "best_match"
                | "updated_date"
                | "created_date"
                | "title_ascending"
                | "year_descending"
                | "average_score"
                | "most_views_7d"
                | "most_views_1mo"
                | "total_views"
                | "most_follows"
        )
    });

    if trimmed_query.is_empty() {
        return vec![build_search_url(None, genre, browse_order, page)];
    }

    vec![
        build_search_url(Some(("keyword", trimmed_query)), genre, None, page),
        build_search_url(Some(("q", trimmed_query)), genre, None, page),
    ]
}

pub fn build_search_page_url(
    query: &str,
    category: Option<&str>,
    popular: bool,
    page: u32,
) -> String {
    let trimmed_query = query.trim();
    let trimmed_category = category.map(str::trim).filter(|value| !value.is_empty());
    let browse_order = browse_order(trimmed_category, popular);

    let mut url = Url::parse(BASE_URL).expect("BASE_URL should be a valid absolute URL");
    {
        let mut path_segments = url
            .path_segments_mut()
            .expect("BASE_URL should support path segments");
        path_segments.pop_if_empty();
        path_segments.push("browse");
    }

    {
        let mut query_pairs = url.query_pairs_mut();
        if !trimmed_query.is_empty() {
            query_pairs.append_pair("q", trimmed_query);
        }
        if let Some(order) = browse_order {
            query_pairs.append_pair("sort", &format!("{}:{}", order.field(), order.direction()));
        } else if !trimmed_query.is_empty() {
            query_pairs.append_pair("sort", "relevance:desc");
        }
        if page > 1 {
            query_pairs.append_pair("page", &page.to_string());
        }
    }

    url.into()
}

pub fn manga_identity(manga_id: i32, hash_id: Option<&str>) -> String {
    match hash_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(hash_id) => format!("{manga_id}|{hash_id}"),
        None => manga_id.to_string(),
    }
}

fn resolve_hash_id(numeric_id: &str) -> Option<String> {
    let url = manga_details_url(numeric_id);
    let json_str = get_json_body(&url).ok()?;
    let response = parse_manga_details_response(&json_str).ok()?;
    response
        .result
        .hash_id
        .filter(|hash_id| !hash_id.trim().is_empty())
}

pub fn manga_details_url(manga_id: &str) -> String {
    let path = format!("/manga/{manga_id}");
    build_api_url(
        ["manga", manga_id],
        &[("_".to_string(), generate_hash(&path))],
    )
}

pub fn title_hash_url(hash_id: &str) -> Option<String> {
    let hash_id = hash_id.trim();
    if hash_id.is_empty() || is_numeric_id(hash_id) {
        return None;
    }

    let mut url = Url::parse(BASE_URL).expect("BASE_URL should be a valid absolute URL");
    {
        let mut path_segments = url
            .path_segments_mut()
            .expect("BASE_URL should support path segments");
        path_segments.pop_if_empty();
        path_segments.push("title");
        path_segments.push(hash_id);
    }

    Some(url.into())
}

pub fn title_url(hash_id: &str, slug: Option<&str>) -> Option<String> {
    let hash_id = hash_id.trim();
    let slug = slug.map(str::trim).filter(|value| !value.is_empty())?;
    if hash_id.is_empty() {
        return None;
    }

    let mut url = Url::parse(BASE_URL).expect("BASE_URL should be a valid absolute URL");
    {
        let mut path_segments = url
            .path_segments_mut()
            .expect("BASE_URL should support path segments");
        path_segments.pop_if_empty();
        path_segments.push("title");
        path_segments.push(&format!("{hash_id}-{slug}"));
    }

    Some(url.into())
}

pub fn chapter_pages_url(chapter_id: &str) -> String {
    let path = format!("/chapters/{chapter_id}");
    build_api_url(
        ["chapters", chapter_id],
        &[("_".to_string(), generate_hash(&path))],
    )
}

pub fn chapter_indexes_url(manga_id: &str) -> String {
    let path = format!("/manga/{manga_id}/chapter-indexes");
    build_api_url(
        ["manga", manga_id, "chapter-indexes"],
        &[("_".to_string(), generate_hash(&path))],
    )
}

pub fn chapter_list_urls(manga_id: &str, limit: u32, page: u32) -> [String; 2] {
    let path = format!("/manga/{manga_id}/chapters");
    let token = generate_hash(&path);
    let limit = limit.to_string();
    let page = page.to_string();

    [
        build_api_url(
            ["manga", manga_id, "chapters"],
            &[
                ("limit".to_string(), limit.clone()),
                ("page".to_string(), page.clone()),
                ("order[number]".to_string(), "desc".to_string()),
                ("_".to_string(), token.clone()),
            ],
        ),
        build_api_url(
            ["manga", manga_id, "chapters"],
            &[
                ("limit".to_string(), limit),
                ("page".to_string(), page),
                ("_".to_string(), token),
            ],
        ),
    ]
}

pub fn chapter_reader_url(
    hash_id: &str,
    slug: Option<&str>,
    chapter_number: f32,
) -> Option<String> {
    if !chapter_number.is_finite() || chapter_number <= 0.0 {
        return None;
    }

    let chapter_number = format_chapter_number(chapter_number);
    let mut url = Url::parse(&title_url(hash_id, slug)?)
        .expect("generated title URL should be a valid absolute URL");
    {
        let mut path_segments = url
            .path_segments_mut()
            .expect("BASE_URL should support path segments");
        path_segments.push(&format!("0-chapter-{chapter_number}"));
    }

    Some(url.into())
}

pub fn chapter_reader_url_from_title_url(
    title_url: &str,
    chapter_id: i32,
    chapter_number: f32,
) -> Option<String> {
    if chapter_id <= 0 || !chapter_number.is_finite() || chapter_number <= 0.0 {
        return None;
    }

    let chapter_number = format_chapter_number(chapter_number);
    let mut url = Url::parse(title_url).ok()?;
    {
        let mut path_segments = url
            .path_segments_mut()
            .expect("title URL should support path segments");
        path_segments.push(&format!("{chapter_id}-chapter-{chapter_number}"));
    }

    Some(url.into())
}

pub fn chapter_id_from_reader_url(url: &str) -> Option<i32> {
    let parsed = Url::parse(url).ok()?;
    let path = parsed.path().trim_matches('/');
    let tail = path.rsplit('/').next()?;
    let chapter_id = tail.split("-chapter-").next()?;
    chapter_id.parse::<i32>().ok()
}

fn format_chapter_number(number: f32) -> String {
    if number.fract() == 0.0 {
        format!("{number:.0}")
    } else {
        let formatted = format!("{number}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn build_search_url(
    search: Option<(&str, &str)>,
    genre: Option<&str>,
    browse_order: Option<SearchOrder>,
    page: u32,
) -> String {
    let page = page.to_string();
    let mut params = vec![("page", page.as_str()), ("limit", "28")];

    if let Some(genre) = genre {
        params.push(("genre", genre));
    }

    if let Some((key, value)) = search {
        params.push((key, value));
    }

    let mut owned_params = params
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();

    if let Some(order) = browse_order {
        owned_params.push((
            format!("order[{}]", order.field()),
            order.direction().to_string(),
        ));
    }

    owned_params.push(("_".to_string(), generate_hash("/manga")));

    build_api_url(["manga"], &owned_params)
}

fn build_api_url<const N: usize>(segments: [&str; N], query: &[(String, String)]) -> String {
    let mut url = Url::parse(API_URL).expect("API_URL should be a valid absolute URL");
    {
        let mut path_segments = url
            .path_segments_mut()
            .expect("API_URL should support path segments");
        path_segments.pop_if_empty();
        for segment in segments {
            path_segments.push(segment);
        }
    }
    if !query.is_empty() {
        let mut query_pairs = url.query_pairs_mut();
        for (key, value) in query {
            query_pairs.append_pair(key, value);
        }
    }
    url.into()
}

#[derive(Clone, Copy)]
enum SearchOrder {
    CreatedAt,
    FollowsTotal,
    UpdatedAt,
    Title,
    Year,
    Score,
    Views7d,
    Views30d,
    ViewsTotal,
}

fn generate_hash(path: &str) -> String {
    let encoded = encode_uri_component(path);
    let signed = HASH_STEPS
        .iter()
        .fold(encoded.into_bytes(), |data, step| match *step {
            HashStep::Cipher(key) => rc4(&decode_hash_key(key), &data),
            HashStep::Mutate(round) => apply_hash_round(&data, round),
        });

    URL_SAFE_NO_PAD.encode(signed)
}

fn encode_uri_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'!'
                | b'~'
                | b'*'
                | b'\''
                | b'('
                | b')'
        ) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

fn decode_hash_key(key: &str) -> Vec<u8> {
    STANDARD.decode(key.as_bytes()).unwrap_or_default()
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }

    let mut s = [0_u8; 256];
    for (index, value) in s.iter_mut().enumerate() {
        *value = u8::try_from(index).expect("index fits into u8");
    }

    let mut j = 0_usize;
    for i in 0..256 {
        j = (j + usize::from(s[i]) + usize::from(key[i % key.len()])) % 256;
        s.swap(i, j);
    }

    let mut i = 0_usize;
    j = 0;
    data.iter()
        .map(|byte| {
            i = (i + 1) % 256;
            j = (j + usize::from(s[i])) % 256;
            s.swap(i, j);
            let key_index = (usize::from(s[i]) + usize::from(s[j])) % 256;
            *byte ^ s[key_index]
        })
        .collect()
}

fn apply_hash_round(data: &[u8], round: HashRound) -> Vec<u8> {
    let mutation_key = decode_hash_key(round.mutation_key);
    let prefix = decode_hash_key(round.prefix);
    let mut out = Vec::with_capacity(data.len() + round.prefix_len);

    for (index, byte) in data.iter().copied().enumerate() {
        if index < round.prefix_len && index < prefix.len() {
            out.push(prefix[index]);
        }

        let value = byte ^ mutation_key_byte(&mutation_key, index);
        out.push(round.mutations[index % round.mutations.len()].apply(value));
    }

    out
}

fn mutation_key_byte(mutation_key: &[u8], index: usize) -> u8 {
    let key_index = index % 32;
    if mutation_key.is_empty() || key_index >= mutation_key.len() {
        0
    } else {
        mutation_key[key_index]
    }
}

impl ByteMutation {
    fn apply(self, value: u8) -> u8 {
        match self {
            Self::Sub47 => value.wrapping_sub(47),
            Self::Sub70 => value.wrapping_sub(70),
            Self::Sub147 => value.wrapping_sub(147),
            Self::Sub181 => value.wrapping_sub(181),
            Self::Sub226 => value.wrapping_sub(226),
            Self::RotateLeft1 => value.rotate_left(1),
            Self::RotateLeft5 => value.rotate_left(5),
            Self::RotateRight2 => value.rotate_right(2),
            Self::RotateRight3 => value.rotate_right(3),
            Self::Xor28 => value ^ 0x1c,
            Self::Xor37 => value ^ 0x25,
            Self::Xor208 => value ^ 0xd0,
        }
    }
}

impl SearchOrder {
    fn field(self) -> &'static str {
        match self {
            Self::CreatedAt => "created_at",
            Self::FollowsTotal => "follows_total",
            Self::UpdatedAt => "chapter_updated_at",
            Self::Title => "title",
            Self::Year => "year",
            Self::Score => "score",
            Self::Views7d => "views_7d",
            Self::Views30d => "views_30d",
            Self::ViewsTotal => "views_total",
        }
    }

    fn direction(self) -> &'static str {
        match self {
            Self::Title => "asc",
            _ => "desc",
        }
    }
}

fn browse_order(category: Option<&str>, popular: bool) -> Option<SearchOrder> {
    if popular {
        return Some(SearchOrder::Score);
    }

    match category.map(str::to_ascii_lowercase).as_deref() {
        Some("updated_date") => Some(SearchOrder::UpdatedAt),
        Some("created_date") => Some(SearchOrder::CreatedAt),
        Some("title_ascending") => Some(SearchOrder::Title),
        Some("year_descending") => Some(SearchOrder::Year),
        Some("average_score") => Some(SearchOrder::Score),
        Some("most_views_7d") => Some(SearchOrder::Views7d),
        Some("most_views_1mo") => Some(SearchOrder::Views30d),
        Some("total_views") => Some(SearchOrder::ViewsTotal),
        Some("most_follows") => Some(SearchOrder::FollowsTotal),
        _ => None,
    }
}
