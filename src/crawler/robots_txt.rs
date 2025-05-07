use reqwest::StatusCode;
use robots_txt::Robots;
use url::Url;

#[derive(Clone)]
pub struct RobotsTxtSource {
    content: String,
    agent: String,
}

impl RobotsTxtSource {
    pub async fn load_from_url(url: &Url, agent: &str) -> anyhow::Result<Self> {
        let mut robots_txt_url = url.clone();
        robots_txt_url.set_path("/robots.txt");
        let robots_response = reqwest::get(robots_txt_url).await?;
        if !robots_response.status().is_success() {
            if robots_response.status() == StatusCode::NOT_FOUND {
                return Ok(Self {
                    content: String::new(),
                    agent: agent.to_owned(),
                });
            }
            return Err(anyhow::anyhow!("An error occurred fetching robots.txt"));
        }
        let content = robots_response.text().await?;
        Ok(Self {
            content,
            agent: agent.to_owned(),
        })
    }

    pub fn view(&self) -> RobotsTxtView<'_> {
        let context = self.content.as_str();
        let robot = Robots::from_str_lossy(context);
        RobotsTxtView {
            content: context,
            robot,
            agent: self.agent.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RobotsTxtView<'a> {
    #[allow(dead_code)]
    content: &'a str,
    robot: Robots<'a>,
    agent: String,
}

impl<'a> RobotsTxtView<'a> {
    pub fn matcher(&self) -> RobotsTxtMatcher<'_> {
        let matcher = robots_txt::matcher::SimpleMatcher::new(
            &self.robot.choose_section(self.agent.as_str()).rules,
        );
        RobotsTxtMatcher { matcher }
    }
}

pub struct RobotsTxtMatcher<'a> {
    matcher: robots_txt::matcher::SimpleMatcher<'a>,
}

impl<'a> RobotsTxtMatcher<'a> {
    pub fn check_path(&self, path: &str) -> bool {
        self.matcher.check_path(path)
    }
}