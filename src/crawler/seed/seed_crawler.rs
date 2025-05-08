use crate::crawler::crawl_error::CrawlError;
use crate::crawler::crawl_summary::CrawlSummary;
use crate::crawler::crawler_config::CrawlerConfig;
use crate::console::crawler_state::CrawlerState;
use crate::crawler::page::PageCrawler;
use crate::crawler::page_summary::PageSummary;
use crate::crawler::seed::progress_reporter::ProgressReporter;
use crate::crawler::robots::RobotsTxtMatcher;
use crate::crawler::robots::RobotsTxtSource;
use crate::crawler::seed::crawl_context::CrawlContext;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use url::Url;

enum PageCrawlOutput {
    NoMoreUrlsToCrawl,
    DeniedByRobotsTxt(Url),
    HttpNotFound(Url),
    HttpError(Url, u16),
    Success(PageSummary),
}

pub struct SeedCrawler<TP>
where
    TP: ProgressReporter,
{
    shutdown_notify: Arc<tokio::sync::Notify>,
    seed: Url,
    progress_reporter: TP,
}

impl<TP> SeedCrawler<TP>
where
    TP: ProgressReporter,
{
    pub fn new(
        shutdown_notify: Arc<tokio::sync::Notify>,
        seed: Url,
        progress_reporter: TP,
    ) -> Self {
        Self {
            shutdown_notify,
            //index,
            seed,
            progress_reporter,
        }
    }

    pub async fn crawl(&self, config: CrawlerConfig) -> anyhow::Result<CrawlSummary> {
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        {
            let shutdown_notify = Arc::clone(&self.shutdown_notify);
            let shutdown_requested = Arc::clone(&shutdown_requested);
            tokio::task::spawn(async move {
                shutdown_notify.notified().await;
                shutdown_requested.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }

        self.progress_reporter.begin();

        let crawl_delay: Option<tokio::time::Duration> = {
            if let Some(requests_per_second) = config.requests_per_second() {
                let crawl_delay_in_ms = (1000.0 / requests_per_second) as u64;
                Some(tokio::time::Duration::from_millis(crawl_delay_in_ms))
            } else {
                None
            }
        };

        let seed_url = self.seed.clone();
        let robots_txt_source = RobotsTxtSource::load_from_url(&seed_url, "rusty-spider").await?;
        let robots_txt_view = robots_txt_source.view();
        let robots_txt_matcher = robots_txt_view.matcher();

        let mut crawl_context = CrawlContext::new();
        crawl_context.add_url_to_crawl(&seed_url);

        self.progress_reporter
            .crawler_state_changed(CrawlerState::Crawling);

        let mut crawl_summary = CrawlSummary::default();
        while !shutdown_requested.load(std::sync::atomic::Ordering::Relaxed)
            && !crawl_context.is_crawling_complete()
        {
            let crawl_progress = crawl_context.progress();
            self.progress_reporter
                .progress_update(crawl_progress.0, crawl_progress.1);

            let output = self
                .crawl_next_url(&robots_txt_matcher, &mut crawl_context)
                .await?;
            let page_summary = match output {
                PageCrawlOutput::Success(page_summary) => Some(page_summary),
                PageCrawlOutput::HttpNotFound(url) => Some(PageSummary::from_status_code(url, 404)),
                PageCrawlOutput::HttpError(url, status_code) => {
                    Some(PageSummary::from_status_code(url, status_code))
                }
                PageCrawlOutput::NoMoreUrlsToCrawl => None,
                PageCrawlOutput::DeniedByRobotsTxt(url) => {
                    Some(PageSummary::from_status_code(url, 403))
                }
            };
            if let Some(page_summary) = page_summary {
                crawl_summary.add_page_summary(page_summary);
            }

            if let Some(crawl_delay) = crawl_delay {
                if !crawl_context.is_crawling_complete() {
                    if shutdown_requested.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }

                    self.progress_reporter
                        .crawler_state_changed(CrawlerState::Paused);
                    tokio::time::sleep(crawl_delay).await;
                    self.progress_reporter
                        .crawler_state_changed(CrawlerState::Crawling);
                }
            }
        }

        self.progress_reporter.end();

        Ok(crawl_summary)
    }

    async fn crawl_next_url(
        &self,
        robots_txt_matcher: &RobotsTxtMatcher<'_>,
        crawl_context: &mut CrawlContext,
    ) -> anyhow::Result<PageCrawlOutput> {
        // Fetch the next URL to crawl
        let url_to_crawl = crawl_context.pop_url_to_crawl();
        if let None = url_to_crawl {
            return Ok(PageCrawlOutput::NoMoreUrlsToCrawl);
        }
        let url_to_crawl = url_to_crawl.unwrap();
        crawl_context.mark_url_as_crawled(&url_to_crawl);

        // Ensure this URL is allowed to be crawled by robots.txt
        if !robots_txt_matcher.check_path(url_to_crawl.path()) {
            return Ok(PageCrawlOutput::DeniedByRobotsTxt(url_to_crawl));
        }

        {
            let msg = format!("Crawling {}", url_to_crawl);
            self.progress_reporter.progress_message(&msg);
        }

        // Fetch the contents of the URL
        let crawl_response = {
            let page_crawler = PageCrawler::new();
            page_crawler.crawl(&url_to_crawl).await
        };
        match crawl_response {
            Ok(crawl_response) => {
                crawl_context.add_urls_to_crawl(&crawl_response.internal_links);

                let page_summary = PageSummary::new(
                    crawl_response.url,
                    crawl_response.status_code,
                    crawl_response.content_type,
                    crawl_response.title,
                    crawl_response.outgoing_links.len(),
                );
                Ok(PageCrawlOutput::Success(page_summary))
            }
            Err(e) => match e {
                CrawlError::HttpError(status_code) => {
                    if status_code == 404 {
                        Ok(PageCrawlOutput::HttpNotFound(url_to_crawl))
                    } else {
                        Ok(PageCrawlOutput::HttpError(url_to_crawl, status_code))
                    }
                }
                _ => Err(anyhow::anyhow!("Crawl error: {}", e)),
            },
        }
    }
}
