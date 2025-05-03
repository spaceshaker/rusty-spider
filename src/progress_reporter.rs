use crate::crawler_state::CrawlerState;

pub trait ProgressReporter {
    fn begin(&self);
    fn progress_update(&self, num_urls_to_crawl: usize, num_urls_crawled: usize);
    fn crawler_state_changed(&self, state: CrawlerState);
    fn end(&self);
}
