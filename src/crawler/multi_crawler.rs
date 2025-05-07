use std::sync::Arc;
use url::Url;
use tokio::task::JoinHandle;
use futures::future::join_all;
use crate::console::console_progress_reporter::ConsoleProcessReporter;
use crate::crawler::crawl_summary::CrawlSummary;
use crate::crawler::SeedCrawler;
use crate::crawler::crawler_config::CrawlerConfig;
use crate::crawler::crawler_progress_reporter::CrawlerProgressReporter;

#[derive(Clone)]
pub struct MultiCrawler {
    shutdown_notify: Arc<tokio::sync::Notify>,
    crawler_config: CrawlerConfig,
    console_process_reporter: ConsoleProcessReporter,
    seeds: Vec<Url>,
}

impl MultiCrawler {
    pub fn new(
        shutdown_notify: Arc<tokio::sync::Notify>,
        crawler_config: CrawlerConfig,
        console_process_reporter: ConsoleProcessReporter,
    ) -> Self {
        Self {
            shutdown_notify,
            crawler_config,
            console_process_reporter,
            seeds: Vec::new(),
        }
    }

    pub fn add_seed(&mut self, seed: Url) {
        self.seeds.push(seed);
    }

    pub async fn run(self) -> anyhow::Result<Vec<CrawlSummary>> {
        let shutdown_notify = Arc::clone(&self.shutdown_notify);
        let console_process_reporter = self.console_process_reporter.clone();
        let crawler_config = self.crawler_config.clone();
        let handles = self
            .seeds
            .iter()
            .cloned()
            .enumerate()
            .map(|(crawler_index, seed)| {
                let shutdown_notify = Arc::clone(&shutdown_notify);
                let console_reporter = console_process_reporter.clone();
                let crawler_config = crawler_config.clone();
                tokio::task::spawn(async move {
                    let progress_reporter = CrawlerProgressReporter::new(
                        crawler_index,
                        seed.clone(),
                        console_reporter.event_tx(),
                    );
                    let seed_crawler =
                        SeedCrawler::new(shutdown_notify, seed.clone(), progress_reporter);
                    let crawl_summary = seed_crawler.crawl(crawler_config).await?;
                    Ok::<CrawlSummary, anyhow::Error>(crawl_summary)
                })
            })
            .collect::<Vec<JoinHandle<anyhow::Result<CrawlSummary>>>>();
        let all_tasks = join_all(handles).await;
        let results: Vec<CrawlSummary> = all_tasks
            .into_iter()
            .filter_map(|task_result| task_result.ok().and_then(|res| res.ok()))
            .collect();
        Ok(results)
    }
}