use serde_json::Value;

/// Keep unrelated assertion values out of failure notifications.
fn recognition_field(item: &str) -> Option<&'static str> {
    let path = item.strip_prefix("JSON ").or_else(|| item.strip_prefix("JSONPath "))?;
    let field =
        path.rsplit(['/', '.', '[']).next()?.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let normalized: String =
        field.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect();
    match normalized.as_str() {
        "tyreserial" | "tireserial" | "tyreno" | "tireno" | "tyrenumber" => Some("胎号"),
        "odometer" | "odometerkm" | "odometerreading" | "mileage" | "mileagekm" => Some("里程"),
        "platenumber" | "licenceplate" | "licenseplate" | "platecode" => Some("车牌"),
        _ => None,
    }
}

pub(crate) struct NoticeCase {
    pub name: String,
    pub outcome: String,
    pub assertions: Value,
    pub status_code: Option<i32>,
}

fn short_text(text: &str, limit: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(limit).collect()
}

fn reading(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(|value| short_text(value.trim_matches('"'), 18))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "未返回".into())
}

fn case_detail(case: &NoticeCase) -> Option<String> {
    let name = short_text(&case.name, 26);
    let status = if case.outcome == "SUCCESS" { "通过" } else { "未通过" };
    let recognized = case
        .assertions
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|assertion| {
            let field = recognition_field(assertion.get("item")?.as_str()?)?;
            let equality = assertion
                .get("condition")
                .and_then(Value::as_str)
                .is_none_or(|condition| condition == "equals");
            Some((field, assertion, equality))
        })
        .max_by_key(|(_, assertion, equality)| {
            (*equality, assertion.get("passed") == Some(&Value::Bool(false)))
        });
    if let Some((field, assertion, equality)) = recognized {
        let actual = reading(assertion.get("actual"));
        if !equality {
            return Some(format!("{name} · {field} {actual}（{status}）"));
        }
        let expected = reading(assertion.get("expected"));
        return Some(format!("{name} · {field} {actual}（预期 {expected}，{status}）"));
    }
    let lower = case.name.to_lowercase();
    let recognition =
        ["ocr", "识别", "胎号", "里程表", "车牌"].iter().any(|word| lower.contains(word));
    if recognition {
        Some(format!("{name} · {status}，报告中查看识别值"))
    } else if case.outcome != "SUCCESS" {
        Some(match case.status_code {
            Some(code) => format!("{name} · {status}（HTTP {code}）"),
            None => format!("{name} · {status}"),
        })
    } else {
        None
    }
}

pub(crate) fn scenario_notice_content(
    status: &str,
    cases: &[NoticeCase],
    report_url: Option<&str>,
) -> String {
    let passed = cases.iter().filter(|case| case.outcome == "SUCCESS").count();
    let passed_run = status == "SUCCESS";
    let details: Vec<_> = [false, true]
        .into_iter()
        .flat_map(|passed| {
            cases
                .iter()
                .filter(move |case| (case.outcome == "SUCCESS") == passed)
                .filter_map(case_detail)
        })
        .take(8)
        .collect();
    let mut text = format!(
        "| 执行结果 | 总数 | 通过 | 失败 |\n| --- | --- | --- | --- |\n| {} | {} | {} | {} |\n",
        if passed_run { "成功" } else { "失败" },
        cases.len(),
        passed,
        cases.len() - passed,
    );
    if let Some(url) = report_url {
        text.push_str(&format!("\n[查看完整测试报告]({url})\n"));
    }
    if !details.is_empty() {
        text.push_str("\n### 用例结果\n");
        for detail in details {
            text.push_str(&format!("- {detail}\n"));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarizes_routine_success_and_keeps_recognition_readings() {
        let ordinary = || NoticeCase {
            name: "车队长问数回归".into(),
            outcome: "SUCCESS".into(),
            assertions: json!([{"item":"JSON /count","actual":"9","expected":"9"}]),
            status_code: Some(200),
        };
        let url = "https://shepherd.example/share/report/example?access=project";
        let content = scenario_notice_content("SUCCESS", &[ordinary()], Some(url));
        assert!(content.contains("| 成功 | 1 | 1 | 0 |"));
        assert!(content.contains(url));
        assert!(!content.contains("### 用例结果"));
        let model = NoticeCase {
            name: "样本 09".into(),
            outcome: "SUCCESS".into(),
            assertions: json!([{"item":"JSON /data/tyre_serial","actual":"\"2354892105\"","expected":"2354892105","passed":true}]),
            status_code: Some(200),
        };
        let content = scenario_notice_content("SUCCESS", &[ordinary(), model], None);
        assert!(content.contains("胎号 2354892105（预期 2354892105，通过）"));
        assert!(!content.contains("车队长问数回归"));
        let odometer = NoticeCase {
            name: "里程样本".into(),
            outcome: "ERROR".into(),
            assertions: json!([
                {"item":"JSONPath $.odometer_km","condition":"regex","actual":"234.00","expected":"[0-9]+","passed":true},
                {"item":"JSON /odometer_km","condition":"equals","actual":"234.00","expected":"235.00","passed":false}
            ]),
            status_code: Some(200),
        };
        let content = scenario_notice_content("ERROR", &[odometer], None);
        assert!(content.contains("里程 234.00（预期 235.00，未通过）"));
        assert!(!content.contains("[0-9]+"));
    }
}
