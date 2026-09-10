use crate::{api::dto::ApiListResponse, app::chapter_pages::DownloadedChapterPageReference};

pub(crate) const DOWNLOADED_CHAPTER_PAGE_ROUTE_PREFIX: &str = "/v1/library/chapters/";
pub(crate) const DOWNLOADED_CHAPTER_PAGE_ROUTE_SEGMENT: &str = "/pages/";

pub(crate) fn downloaded_chapter_page_urls(
    references: &ApiListResponse<DownloadedChapterPageReference>,
) -> ApiListResponse<String> {
    ApiListResponse::new(
        references
            .items
            .iter()
            .map(downloaded_chapter_page_url)
            .collect(),
    )
}

fn downloaded_chapter_page_url(reference: &DownloadedChapterPageReference) -> String {
    format!(
        "{}{}{}{}",
        DOWNLOADED_CHAPTER_PAGE_ROUTE_PREFIX,
        reference.chapter_id,
        DOWNLOADED_CHAPTER_PAGE_ROUTE_SEGMENT,
        reference.index
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_chapter_page_urls_match_external_reader_contract() {
        let response = downloaded_chapter_page_urls(&ApiListResponse::new(vec![
            DownloadedChapterPageReference {
                index: 0,
                chapter_id: "chapter-1".to_string(),
            },
            DownloadedChapterPageReference {
                index: 12,
                chapter_id: "chapter-1".to_string(),
            },
        ]));

        assert_eq!(
            response.items,
            [
                "/v1/library/chapters/chapter-1/pages/0",
                "/v1/library/chapters/chapter-1/pages/12",
            ]
        );
    }
}
