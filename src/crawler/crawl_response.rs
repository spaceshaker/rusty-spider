use url::Url;

#[derive(Debug, Clone)]
pub struct CrawlResponse {
    pub url: Url,
    pub status_code: u16,
    pub content_type: String,
    pub title: String,
    pub outgoing_links: Vec<Url>,
    pub internal_links: Vec<Url>,
}