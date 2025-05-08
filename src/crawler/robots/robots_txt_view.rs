use crate::crawler::robots::robots_txt_matcher::RobotsTxtMatcher;
use robots_txt::Robots;

#[derive(Clone)]
pub struct RobotsTxtView<'a> {
    #[allow(dead_code)]
    content: &'a str,
    robot: Robots<'a>,
    agent: String,
}

impl<'a> RobotsTxtView<'a> {
    pub fn new(content: &'a str, robot: Robots<'a>, agent: String) -> Self {
        Self {
            content,
            robot,
            agent,
        }
    }

    pub fn matcher(&self) -> RobotsTxtMatcher<'_> {
        let matcher = robots_txt::matcher::SimpleMatcher::new(
            &self.robot.choose_section(self.agent.as_str()).rules,
        );
        RobotsTxtMatcher::new(matcher)
    }
}
