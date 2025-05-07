use url::Url;
use crate::crawler::crawler_progress_event::CrawlerProcessEvent;
use crate::crawler::crawler_state::CrawlerState;
use crate::crawler::progress_reporter::ProgressReporter;

#[derive(Clone)]
pub struct CrawlerProgressReporter {
    index: usize,
    url: Url,
    event_tx: tokio::sync::mpsc::Sender<CrawlerProcessEvent>,
}

impl CrawlerProgressReporter {
    pub fn new(index: usize, url: Url, event_tx: tokio::sync::mpsc::Sender<CrawlerProcessEvent>) -> Self {
        Self { index, url, event_tx }
    }
}

impl ProgressReporter for CrawlerProgressReporter {
    fn begin(&self) {
        futures::executor::block_on(async {
            let _ = self.event_tx.send(CrawlerProcessEvent::Begin {
                crawler_index: self.index,
                url: self.url.clone(),
            }).await;    
        })
    }

    fn progress_update(&self, num_urls_to_crawl: usize, num_urls_crawled: usize) {
        futures::executor::block_on(async {
            let _ = self.event_tx.send(CrawlerProcessEvent::ProgressUpdate {
                crawler_index: self.index,
                num_urls_to_crawl,
                num_urls_crawled,
            }).await;
        })
    }

    fn progress_message(&self, message: &str) {
        futures::executor::block_on(async {
            let _ = self.event_tx.send(CrawlerProcessEvent::ProgressMessage {
                crawler_index: self.index,
                message: message.to_owned(),
            }).await;
        })
    }

    fn crawler_state_changed(&self, state: CrawlerState) {
        futures::executor::block_on(async {
            let _ = self
                .event_tx
                .send(CrawlerProcessEvent::CrawlerStateChanged {
                    crawler_index: self.index,
                    state,
                }).await;
        })
    }

    fn end(&self) {
        futures::executor::block_on(async {
            let _ = self.event_tx.send(CrawlerProcessEvent::End {
                crawler_index: self.index,
            }).await;
        })
    }
}