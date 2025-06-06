use url::Url;
use crate::console::crawler_state::CrawlerState;

#[derive(Debug, Clone)]
pub enum CrawlerProcessEvent {
    Begin {
        crawler_index: usize,
        url: Url,
    },
    ProgressUpdate {
        crawler_index: usize,
        num_urls_to_crawl: usize,
        num_urls_crawled: usize,
    },
    ProgressMessage {
        crawler_index: usize,
        message: String,
    },
    CrawlerStateChanged {
        crawler_index: usize,
        state: CrawlerState,
    },
    End {
        crawler_index: usize,
    },
}