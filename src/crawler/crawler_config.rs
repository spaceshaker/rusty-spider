#[derive(Clone)]
pub struct CrawlerConfig {
    max_pages: usize,
    max_depth: usize,
    requests_per_second: Option<f64>,
}

impl CrawlerConfig {
    pub fn new(max_pages: usize, max_depth: usize, requests_per_second: Option<f64>) -> Self {
        Self {
            max_pages,
            max_depth,
            requests_per_second,
        }
    }

    #[allow(dead_code)]
    pub fn max_pages(&self) -> usize {
        self.max_pages
    }

    #[allow(dead_code)]
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    pub fn requests_per_second(&self) -> Option<f64> {
        self.requests_per_second
    }
}
