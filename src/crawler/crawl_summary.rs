use crate::crawler::page_summary::PageSummary;

#[derive(Debug, Clone)]
pub struct CrawlSummary {
    crawl_summaries: Vec<PageSummary>,
}

impl CrawlSummary {
    pub fn new(crawl_summaries: Vec<PageSummary>) -> Self {
        Self { crawl_summaries }
    }

    pub fn page_summaries(&self) -> &[PageSummary] {
        &self.crawl_summaries
    }
    
    pub fn add_page_summary(&mut self, page_summary: PageSummary) {
        self.crawl_summaries.push(page_summary);
    }
}

impl Default for CrawlSummary {
    fn default() -> Self {
        CrawlSummary::new(Vec::new())
    }
}