use std::collections::HashSet;
use url::Url;

#[derive(Clone)]
pub struct CrawlContext {
    urls_to_crawl: HashSet<Url>,
    urls_already_crawled: HashSet<Url>,
}

impl CrawlContext {
    pub fn new() -> Self {
        Self {
            urls_to_crawl: HashSet::new(),
            urls_already_crawled: HashSet::new(),
        }
    }

    pub fn add_url_to_crawl(&mut self, url: &Url) {
        let stripped_url = self.strip_url(url);
        if !self.urls_already_crawled.contains(&stripped_url) {
            self.urls_to_crawl.insert(stripped_url);
        }
    }

    pub fn add_urls_to_crawl(&mut self, urls: &[Url]) {
        for url in urls {
            self.add_url_to_crawl(url);
        }
    }

    pub fn pop_url_to_crawl(&mut self) -> Option<Url> {
        self.urls_to_crawl.iter().next().cloned().and_then(|url| {self.urls_to_crawl.take(&url)})
    }

    pub fn mark_url_as_crawled(&mut self, url: &Url) {
        let stripped_url = self.strip_url(url);
        self.urls_to_crawl.remove(&stripped_url);
        self.urls_already_crawled.insert(stripped_url);
    }

    pub fn is_crawling_complete(&self) -> bool {
        self.urls_to_crawl.is_empty()
    }

    pub fn progress(&self) -> (usize, usize) {
        let num_urls_to_crawl = self.urls_to_crawl.len();
        let num_urls_crawled = self.urls_already_crawled.len();
        (num_urls_to_crawl, num_urls_crawled)
    }

    /// Strips the URL of its fragment and query components.
    fn strip_url(&self, url: &Url) -> Url {
        let mut stripped_url = url.clone();
        stripped_url.set_fragment(None);
        stripped_url.set_query(None);
        stripped_url
    }
}

impl Default for CrawlContext {
    fn default() -> Self {
        Self::new()
    }
}