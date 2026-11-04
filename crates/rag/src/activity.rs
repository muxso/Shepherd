//! Deterministic Git activity intent, Shanghai dates and exact author matching.
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityWindow {
    pub from: NaiveDate,
    pub through: NaiveDate,
}
impl ActivityWindow {
    pub fn parse(question: &str, now_ms: i64) -> Option<Self> {
        let q = question.to_lowercase();
        if ["怎么实现", "如何实现", "实现原理", "调用链", "依赖关系"].iter().any(|s| q.contains(s))
        {
            return None;
        }

        if ![
            "干了",
            "干嘛",
            "干什么",
            "干啥",
            "做了",
            "做什么",
            "做啥",
            "改了",
            "日报",
            "周报",
            "每天",
            "每日",
            "工作总结",
            "工作内容",
            "提交记录",
            "提交历史",
            "提交了",
            "有哪些提交",
            "git log",
            "daily",
            "worked on",
            "activity",
        ]
        .iter()
        .any(|s| q.contains(s))
        {
            return None;
        }
        let tz = FixedOffset::east_opt(8 * 3600)?;
        let today = Utc.timestamp_millis_opt(now_ms).single()?.with_timezone(&tz).date_naive();
        let normalized = q.replace(['年', '月'], "-").replace('日', "").replace('/', "-");
        let dates: Vec<_> = normalized
            .char_indices()
            .filter(|(_, c)| c.is_ascii_digit())
            .filter_map(|(i, _)| normalized.get(i..i + 10))
            .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .collect();
        let (from, through) = if let Some(first) = dates.first() {
            (*first, *dates.last().unwrap_or(first))
        } else if let Some(month) = normalized
            .char_indices()
            .filter(|(_, c)| c.is_ascii_digit())
            .filter_map(|(i, _)| normalized.get(i..i + 7))
            .find_map(|s| NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").ok())
        {
            (month, next_month(month)? - Duration::days(1))
        } else if let Some((month, day)) = partial_month(&q) {
            let start = NaiveDate::from_ymd_opt(today.year(), month, day.unwrap_or(1))?;
            (start, if day.is_some() { start } else { next_month(start)? - Duration::days(1) })
        } else if q.contains("昨天") || q.contains("yesterday") {
            (today - Duration::days(1), today - Duration::days(1))
        } else if q.contains("今天") || q.contains("today") {
            (today, today)
        } else if q.contains("上周") {
            let end = today - Duration::days(i64::from(today.weekday().num_days_from_monday()) + 1);
            (end - Duration::days(6), end)
        } else if q.contains("本周") {
            (today - Duration::days(i64::from(today.weekday().num_days_from_monday())), today)
        } else if q.contains("上个月") || q.contains("上月") {
            let end = today.with_day(1)? - Duration::days(1);
            (end.with_day(1)?, end)
        } else if q.contains("本月") || q.contains("这个月") {
            (today.with_day(1)?, today)
        } else if q.contains("30天") || q.contains("30 天") {
            (today - Duration::days(29), today)
        } else {
            (today - Duration::days(6), today)
        };
        Some(Self { from, through })
    }
    pub fn includes(&self, date: &str) -> bool {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .is_ok_and(|d| d >= self.from && d <= self.through)
    }
}
fn partial_month(q: &str) -> Option<(u32, Option<u32>)> {
    let (before, after) = q.split_once('月')?;
    let digits: String = before
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let month = digits.parse::<u32>().ok().filter(|m| (1..=12).contains(m))?;
    let day =
        after.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<u32>().ok();
    Some((month, day))
}
fn next_month(date: NaiveDate) -> Option<NaiveDate> {
    if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    }
}

#[derive(Debug, Deserialize)]
pub struct ActivityMetadata {
    pub author: String,
    pub date: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}
impl ActivityMetadata {
    pub fn matches_author(&self, question: &str) -> bool {
        std::iter::once(&self.author).chain(&self.aliases).any(|s| author_mentioned(question, s))
    }
}
pub fn author_mentioned(question: &str, name: &str) -> bool {
    if name.trim().chars().count() < 2 {
        return false;
    }
    let q = question.to_lowercase();
    let name = name.to_lowercase();
    q.match_indices(&name).any(|(i, _)| {
        if !name.is_ascii() {
            return true;
        }
        let word =
            |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '.' || c == '-';
        !q[..i].chars().next_back().is_some_and(word)
            && !q[i + name.len()..].chars().next().is_some_and(word)
    })
}
pub fn team_mentioned(question: &str) -> bool {
    ["所有人", "团队", "大家", "我们", "每个人", "谁", "all authors", "team"]
        .iter()
        .any(|s| question.to_lowercase().contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> i64 {
        chrono::DateTime::parse_from_rfc3339("2026-11-01T00:30:00+08:00")
            .expect("clock")
            .timestamp_millis()
    }
    #[test]
    fn dates_use_shanghai_calendar() {
        for (q, from, to) in [
            ("王震昨天干了什么", "2026-10-31", "2026-10-31"),
            ("王震10月19日干了什么", "2026-10-19", "2026-10-19"),
            ("王震10月每天在干嘛", "2026-10-01", "2026-10-31"),
            ("王震2026年10月每天在干嘛", "2026-10-01", "2026-10-31"),
            ("张三2026-10-19到2026-10-22的提交记录", "2026-10-19", "2026-10-22"),
            ("团队本周工作总结", "2026-10-26", "2026-11-01"),
        ] {
            let w = ActivityWindow::parse(q, now()).expect("window");
            assert_eq!(w.from.to_string(), from);
            assert_eq!(w.through.to_string(), to);
        }
        assert!(ActivityWindow::parse("这个接口怎么实现", now()).is_none());
        assert!(ActivityWindow::parse("每天定时任务如何实现", now()).is_none());
    }
    #[test]
    fn aliases_are_exact_not_substrings() {
        assert!(author_mentioned("cxl昨天干了什么", "cxl"));
        assert!(!author_mentioned("client每天做什么", "li"));
        assert!(author_mentioned("王震每天干嘛", "王震"));
        assert!(!author_mentioned("张三昨天干嘛", "王震"));
    }
}
