use crate::crawler_progress_event::CrawlerProcessEvent;
use crate::crawler_state::CrawlerState;
use crossterm::{QueueableCommand, execute, queue, ExecutableCommand};
use std::collections::HashMap;
use std::io::{Write, stdout, Stdout};
use std::sync::Arc;
use tokio::select;
use url::Url;

struct CrawlerInfo {
    index: usize,
    url: Url,
    num_urls_to_crawl: usize,
    num_urls_crawled: usize,
    state: CrawlerState,
    message: Option<String>,
}

struct ConsoleState {
    stdout: Stdout,
    crawlers: HashMap<usize, CrawlerInfo>,
}

#[derive(Clone)]
pub struct ConsoleProcessReporter {
    event_tx: tokio::sync::mpsc::Sender<CrawlerProcessEvent>,
    shutdown_notify: Arc<tokio::sync::Notify>,
}

impl ConsoleProcessReporter {
    pub fn new() -> Self {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<CrawlerProcessEvent>(100);

        let shutdown_notify = Arc::new(tokio::sync::Notify::new());
        {
            let shutdown_notify = Arc::clone(&shutdown_notify);
            tokio::task::spawn(async move {
                let mut console_state = ConsoleState {
                    stdout: stdout(),
                    crawlers: HashMap::new(),
                };
                
                let _ = ConsoleProcessReporter::console_setup(&mut console_state).await;
                
                let mut shutdown_requested = false;
                while !shutdown_requested {
                    select! {
                        progress_event = event_rx.recv() => {
                            match progress_event {
                                Some(progress_event) => {
                                    let _ = ConsoleProcessReporter::handle_event(progress_event, &mut console_state).await;
                                    let _ = ConsoleProcessReporter::console_redraw(&console_state).await;
                                },
                                None => {
                                    shutdown_requested = true;
                                }
                            }
                        }
                        _ = shutdown_notify.notified() => {
                            shutdown_requested = true;
                        }
                    }
                }
                
                let _ = ConsoleProcessReporter::console_teardown(&mut console_state).await;
            });
        }
        Self {
            event_tx,
            shutdown_notify,
        }
    }

    pub fn event_tx(&self) -> tokio::sync::mpsc::Sender<CrawlerProcessEvent> {
        self.event_tx.clone()
    }
    
    async fn console_setup(state: &mut ConsoleState) -> anyhow::Result<()> {
        let mut stdout = &state.stdout;
        crossterm::terminal::enable_raw_mode()?;
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        stdout.execute(crossterm::cursor::Hide)?;
        stdout.execute(crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        Ok(())
    }
    
    async fn console_teardown(state: &mut ConsoleState,) -> anyhow::Result<()> {
        let mut stdout = &state.stdout;
        stdout.execute(crossterm::cursor::Show)?;
        stdout.execute(crossterm::terminal::LeaveAlternateScreen)?;
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }

    async fn console_redraw(state: &ConsoleState) -> anyhow::Result<()> {
        let mut crawler_info = state.crawlers.values().collect::<Vec<&CrawlerInfo>>();
        crawler_info.sort_by(|lhs, rhs| lhs.index.cmp(&rhs.index));

        let mut stdout = &state.stdout;
        stdout.queue(crossterm::cursor::SavePosition)?;

        for (index, crawler_info) in crawler_info.iter().enumerate() {
            if index > 0 {
                queue!(stdout, crossterm::cursor::MoveToNextLine(2))?;
            }
            
            let state_str = {
                match crawler_info.state {
                    CrawlerState::Crawling => "Crawling",
                    CrawlerState::Paused => "Paused",
                }
            };

            queue!(
                stdout,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine),
                crossterm::style::Print(format!("Crawling: {} ({})", crawler_info.url.to_owned(), state_str)),
                crossterm::cursor::MoveToNextLine(1),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine),
                crossterm::style::Print(format!(
                    "   # URLs Remaining: {}, # URLS Crawled: {}",
                    crawler_info.num_urls_to_crawl,
                    crawler_info.num_urls_crawled
                )),
            )?;
            
            if let Some(message) = &crawler_info.message {
                queue!(
                    stdout,
                    crossterm::style::Print(format!(", Message: {}", message))
                )?;
            }
        }
        stdout.queue(crossterm::cursor::RestorePosition)?;
        stdout.flush()?;
        Ok(())
    }

    async fn handle_event(
        event: CrawlerProcessEvent,
        state: &mut ConsoleState,
    ) -> anyhow::Result<()> {
        match event {
            CrawlerProcessEvent::Begin { crawler_index, url } => {
                state.crawlers.insert(
                    crawler_index,
                    CrawlerInfo {
                        index: crawler_index,
                        url: url.clone(),
                        num_urls_to_crawl: 0,
                        num_urls_crawled: 0,
                        state: CrawlerState::Paused,
                        message: None,
                    },
                );
            }
            CrawlerProcessEvent::ProgressUpdate {
                crawler_index,
                num_urls_crawled,
                num_urls_to_crawl,
            } => {
                if let Some(crawler_info) = state.crawlers.get_mut(&crawler_index) {
                    crawler_info.num_urls_crawled = num_urls_crawled;
                    crawler_info.num_urls_to_crawl = num_urls_to_crawl;
                }
            }
            CrawlerProcessEvent::CrawlerStateChanged {
                crawler_index,
                state: crawler_state,
            } => {
                if let Some(crawler_info) = state.crawlers.get_mut(&crawler_index) {
                    crawler_info.state = crawler_state;
                }
            }
            CrawlerProcessEvent::End { crawler_index } => {
                state.crawlers.remove(&crawler_index);
            }
        }
        Ok(())
    }
}

impl Drop for ConsoleProcessReporter {
    fn drop(&mut self) {
        self.shutdown_notify.notify_waiters();
    }
}
