use url::Url;

#[derive(Debug, Clone)]
pub struct CrawlRequest {
    pub url: Url,
}