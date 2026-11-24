use super::NewNotice;

/// A compact test report notification and its authenticated report link.
#[derive(Debug, Clone)]
pub struct ReportCard {
    pub title: String,
    pub project: String,
    pub template_summary: String,
    pub status: &'static str,
    pub failed: bool,
    pub counts: Option<[u64; 3]>,
    pub operator: String,
    pub time: String,
    pub details: Vec<String>,
    pub url: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReportContext {
    pub project: String,
    pub scenario: String,
    pub trigger: String,
    pub changed_by: String,
    pub creator: String,
    pub followers: String,
}

impl ReportContext {
    pub fn interpolate(&self, template: &str, title: &str, content: &str, time: &str) -> String {
        let mut parts = template.split("${");
        let mut output = parts.next().unwrap_or_default().to_owned();
        for part in parts {
            if let Some((key, rest)) = part.split_once('}') {
                let value = match key {
                    "project" => Some(self.project.as_str()),
                    "scenario" => Some(self.scenario.as_str()),
                    "trigger" | "operator" => Some(self.trigger.as_str()),
                    "changedBy" => Some(self.changed_by.as_str()),
                    "creator" => Some(self.creator.as_str()),
                    "followers" => Some(self.followers.as_str()),
                    "title" => Some(title),
                    "content" => Some(content),
                    "time" => Some(time),
                    _ => None,
                };
                if let Some(value) = value {
                    output.push_str(if value.is_empty() { "—" } else { value });
                    output.push_str(rest);
                    continue;
                }
            }
            output.push_str("${");
            output.push_str(part);
        }
        output
    }

    pub fn uses_context(template: &str) -> bool {
        ["${project}", "${scenario}", "${trigger}", "${changedBy}", "${creator}", "${followers}"]
            .iter()
            .any(|key| template.contains(key))
    }
}

impl ReportCard {
    pub fn from_notice(notice: &NewNotice, base_url: Option<&str>, time: &str) -> Option<Self> {
        let scenario = match notice.event_type.as_str() {
            "SCENARIO_RUN_SUCCEEDED" | "SCENARIO_RUN_FAILED" => true,
            "PLAN_RUN_FINISHED" | "PLAN_SCHEDULE_FAILED" => false,
            _ => return None,
        };
        let counts = if scenario {
            notice.content.lines().find_map(scenario_counts)
        } else {
            plan_counts(&notice.content)
        };
        let failed = notice.event_type.ends_with("_FAILED") || counts.is_some_and(|row| row[2] > 0);
        let report_url = if scenario {
            notice.content.lines().find_map(scenario_report_url).map(str::to_owned)
        } else {
            None
        };
        let url = report_url.or_else(|| {
            let base = base_url?.trim_end_matches('/');
            let project = path_value(&notice.project_id);
            let id = path_value(&notice.resource_id);
            if scenario {
                Some(format!("{base}/p/{project}/api/scenario?scenario={id}"))
            } else {
                Some(format!("{base}/share/report/{id}?access=project&type=plan"))
            }
        })?;
        let status = match (scenario, failed) {
            (true, true) => "场景执行失败",
            (true, false) => "场景执行成功",
            (false, true) => "测试计划执行失败",
            (false, false) => "测试计划执行完成",
        };
        Some(Self {
            project: String::new(),
            template_summary: String::new(),
            title: notice
                .title
                .split_once('｜')
                .filter(|(label, _)| scenario && matches!(*label, "场景执行成功" | "场景执行失败"))
                .map_or_else(|| notice.title.clone(), |(_, title)| title.to_owned()),
            status,
            failed,
            counts,
            operator: notice.operator.clone(),
            time: time.to_owned(),
            details: notice
                .content
                .lines()
                .skip_while(|line| *line != "### 用例结果")
                .skip(1)
                .map_while(|line| line.strip_prefix("- "))
                .take(2)
                .map(str::to_owned)
                .collect(),
            url,
        })
    }
}

fn path_value(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn scenario_counts(line: &str) -> Option<[u64; 3]> {
    let line = line.trim().strip_prefix('|')?.strip_suffix('|')?;
    let mut cells = line.split('|').skip(1).map(|value| value.trim().parse::<u64>());
    let counts = [cells.next()?.ok()?, cells.next()?.ok()?, cells.next()?.ok()?];
    (cells.next().is_none() && counts[1].checked_add(counts[2]) == Some(counts[0]))
        .then_some(counts)
}

fn plan_counts(content: &str) -> Option<[u64; 3]> {
    let (passed, rest) = content.split_once('/')?;
    let (total, failed) = rest.split_once(" passed, ")?;
    Some([total.parse().ok()?, passed.parse().ok()?, failed.strip_suffix(" failed")?.parse().ok()?])
}

fn scenario_report_url(line: &str) -> Option<&str> {
    let url = line.trim().strip_prefix("[查看完整测试报告](")?.strip_suffix(')')?;
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || !(url.contains("/ocr-report.html?")
            || url.contains("/share/report/") && url.contains("?access=project"))
        || url.contains(char::is_whitespace)
    {
        return None;
    }
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_template_resolves_people_and_preserves_literal_values() {
        let context = ReportContext {
            project: "车联网".into(),
            scenario: "胎号识别".into(),
            trigger: "王飞".into(),
            changed_by: "李博学".into(),
            creator: "岳星月".into(),
            followers: "王飞、岳星月".into(),
        };
        let template = "${project} / ${scenario} / ${trigger} / ${changedBy} / ${followers} / ${creator} / ${content} / ${unknown}";
        assert!(ReportContext::uses_context(template));
        assert_eq!(
            context.interpolate(template, "", "${project}", ""),
            "车联网 / 胎号识别 / 王飞 / 李博学 / 王飞、岳星月 / 岳星月 / ${project} / ${unknown}"
        );
    }

    #[test]
    fn scenario_card_uses_metrics_and_authenticated_report_link() {
        let mut notice = NewNotice {
            event_type: "SCENARIO_RUN_FAILED".into(),
            title: "胎号识别".into(),
            content: "| ❌ 失败 | 1000 | 998 | 2 |\n\n[查看完整测试报告](http://shepherd.test/ocr-report.html?projectId=p&reportId=r)".into(),
            project_id: "p".into(),
            resource_id: "s".into(),
            operator: String::new(),
            category: "API".into(),
            resource_type: "SCENARIO".into(),
            receivers: vec![],
            at_mention: false,
        };
        let card = ReportCard::from_notice(&notice, None, "2026-11-21 15:55:18").unwrap();
        assert_eq!(card.counts, Some([1000, 998, 2]));
        assert!(card.failed);
        assert_eq!(card.url, "http://shepherd.test/ocr-report.html?projectId=p&reportId=r");

        notice.event_type = "PLAN_RUN_FINISHED".into();
        notice.content = "2/3 passed, 1 failed".into();
        let card =
            ReportCard::from_notice(&notice, Some("https://shepherd.test/"), "2026-11-21 15:55:18")
                .unwrap();
        assert_eq!(card.counts, Some([3, 2, 1]));
        assert_eq!(card.url, "https://shepherd.test/share/report/s?access=project&type=plan");
    }
}
