use clap::Parser;
use console::console_progress_reporter::ConsoleProcessReporter;
use crawler::crawl_summary::CrawlSummary;
use crawler::crawler_config::CrawlerConfig;
use crawler::multi_crawler::MultiCrawler;
use std::process;
use std::sync::Arc;
use url::Url;

mod crawler;
mod console;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CommandLineArgs {
    /// Seed URLs to start crawling from
    #[arg(long, value_name = "URL")]
    seed: Vec<String>,

    /// Maximum number of pages to crawl
    #[arg(long, default_value_t = 1000)]
    max_pages: usize,

    /// Maximum depth to crawl
    #[arg(long, default_value_t = 4)]
    max_depth: usize,

    /// Rate limit for crawling (requests per second)
    #[arg(long)]
    rate: Option<f64>,
}

async fn main_impl(args: &CommandLineArgs) -> anyhow::Result<()> {
    let crawler_config = CrawlerConfig::new(args.max_pages, args.max_depth, args.rate);

    // Set up a shutdown signal handler
    let shutdown_notify = Arc::new(tokio::sync::Notify::new());
    {
        let shutdown_notify = Arc::clone(&shutdown_notify);
        ctrlc::set_handler(move || {
            println!("Received Ctrl+C, shutting down...");
            shutdown_notify.notify_waiters();
        })?;
    }

    // Run the crawlers for all seeds
    let crawl_summaries = {
        let console_reporter = ConsoleProcessReporter::new();
        let _console_reporter_task = {
            let shutdown_notify = Arc::clone(&shutdown_notify);
            let mut console_reporter = console_reporter.clone();
            tokio::task::spawn(async move {
                console_reporter.run(shutdown_notify).await.unwrap();
            })
        };

        let mut multi_crawler = MultiCrawler::new(
            shutdown_notify.clone(),
            crawler_config.clone(),
            console_reporter.clone(),
        );
        for seed_str in &args.seed {
            let seed_url = Url::parse(seed_str)?;
            multi_crawler.add_seed(seed_url);
        }
        let multi_crawler_handle = tokio::task::spawn(async move {
            let results = multi_crawler.run().await?;
            Ok::<Vec<CrawlSummary>, anyhow::Error>(results)
        });

        multi_crawler_handle.await??
    };

    // Summarize the results
    for crawl_summary in crawl_summaries {
        for page_summary in crawl_summary.page_summaries() {
            println!(
                "{}, {}, {}, {}, {}",
                page_summary.url,
                page_summary.status_code,
                page_summary.content_type,
                page_summary.title,
                page_summary.num_outgoing_links
            );
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let args = CommandLineArgs::parse();

    if let Err(e) = main_impl(&args).await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
