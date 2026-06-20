//! 测试计划报告渲染(纯函数,零 IO):由统计 + 逐用例明细生成自包含 HTML。
//!
//! 报告含三段:概览(总数/执行率/通过率/报告总耗时/断言通过率)、状态分布、
//! 逐用例明细(可展开看断言表 + 响应体)。渲染与 IO 解耦,可穷举单测。

use crate::application::PlanStatistics;
use crate::domain::{AssertionResult, CaseStatus, PlanCase, StepResult};

/// HTML 转义(防内容里的特殊字符破坏页面)。
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 状态 → (中文标签, 文字色, 底色)。
fn badge(status: CaseStatus) -> (&'static str, &'static str, &'static str) {
    match status {
        CaseStatus::Success => ("通过", "#2e7d32", "#e8f5e9"),
        CaseStatus::Error => ("失败", "#c62828", "#ffebee"),
        CaseStatus::FakeError => ("误报", "#ef6c00", "#fff3e0"),
        CaseStatus::Block => ("阻塞", "#455a64", "#eceff1"),
        CaseStatus::Pending => ("未执行", "#757575", "#f5f5f5"),
    }
}

/// 断言表(无断言返回空)。
fn assertion_table(a: &[AssertionResult]) -> String {
    if a.is_empty() {
        return String::new();
    }
    let rows: String = a
        .iter()
        .map(|a| {
            let (st, sc) = if a.passed { ("成功", "#2e7d32") } else { ("失败", "#c62828") };
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td style=\"color:{sc}\">{st}</td><td>{}</td></tr>",
                escape(&a.item), escape(&a.actual), escape(&a.condition), escape(&a.expected), escape(&a.reason),
            )
        })
        .collect();
    format!(
        "<div class=\"sec\">断言</div><table class=\"assert\"><tr>\
         <th>断言项</th><th>返回值</th><th>匹配条件</th><th>匹配值</th><th>状态</th><th>原因</th></tr>{rows}</table>"
    )
}

/// 头部键值表(响应头/请求头)。
fn headers_table(title: &str, h: &[(String, String)]) -> String {
    if h.is_empty() {
        return String::new();
    }
    let rows: String = h
        .iter()
        .map(|(k, v)| format!("<tr><td>{}</td><td>{}</td></tr>", escape(k), escape(v)))
        .collect();
    format!("<div class=\"sec\">{}</div><table class=\"assert\"><tr><th>名称</th><th>值</th></tr>{rows}</table>", escape(title))
}

/// 递归渲染一个场景步骤(及其子步骤)。
fn render_step(s: &StepResult) -> String {
    let (label, color, bg) = badge(s.status);
    let code = s.status_code.map(|c| format!("状态码 {c}　")).unwrap_or_default();
    let kind = escape(&s.kind);
    let name = escape(&s.name);
    let assert = assertion_table(&s.assertions);
    let children: String = s.children.iter().map(render_step).collect();
    let inner = if assert.is_empty() && children.is_empty() {
        String::new()
    } else {
        format!("<div class=\"stepbody\">{assert}{children}</div>")
    };
    format!(
        r#"<details class="step"><summary><span class="kind">{kind}</span><span class="sname">{name}</span>
 <span class="badge" style="color:{color};background:{bg}">{label}</span>
 <span class="meta">{code}响应时间 {lat} ms</span></summary>{inner}</details>"#,
        lat = s.latency_ms,
    )
}

/// 渲染单条用例(summary 行 + 可展开明细)。
fn render_case(idx: usize, c: &PlanCase) -> String {
    let (label, color, bg) = badge(c.status);
    let name = escape(&c.name);
    let case_id = escape(&c.case_id);
    let meta = c
        .result
        .as_ref()
        .map(|r| {
            let code = r.status_code.map(|s| format!("状态码 {s}　")).unwrap_or_default();
            format!("{code}响应时间 {} ms　响应大小 {} bytes", r.latency_ms, r.response_size)
        })
        .unwrap_or_default();

    // 步骤树(场景用例)/ 断言表 / 响应头 / 响应体 / 实际请求
    let (steps_html, assertions_html, headers_html, body_html, request_html) = match c.result.as_ref()
    {
        None => (String::new(), String::new(), String::new(), String::new(), String::new()),
        Some(r) => {
            let steps = if r.steps.is_empty() {
                String::new()
            } else {
                let inner: String = r.steps.iter().map(render_step).collect();
                format!("<div class=\"sec\">步骤</div>{inner}")
            };
            let body = r
                .body
                .as_ref()
                .filter(|b| !b.is_empty())
                .map(|b| format!("<div class=\"sec\">响应体</div><pre>{}</pre>", escape(b)))
                .unwrap_or_default();
            let request = r
                .request
                .as_ref()
                .map(|req| {
                    let hdr = headers_table("请求头", &req.headers);
                    let b = req
                        .body
                        .as_ref()
                        .filter(|x| !x.is_empty())
                        .map(|x| format!("<pre>{}</pre>", escape(x)))
                        .unwrap_or_default();
                    format!(
                        "<div class=\"sec\">实际请求</div><pre>{} {}</pre>{hdr}{b}",
                        escape(&req.method),
                        escape(&req.url),
                    )
                })
                .unwrap_or_default();
            (
                steps,
                assertion_table(&r.assertions),
                headers_table("响应头", &r.response_headers),
                body,
                request,
            )
        }
    };

    let detail = if steps_html.is_empty()
        && assertions_html.is_empty()
        && headers_html.is_empty()
        && body_html.is_empty()
        && request_html.is_empty()
    {
        "<div class=\"empty\">无执行明细</div>".to_string()
    } else {
        format!("{steps_html}{assertions_html}{body_html}{headers_html}{request_html}")
    };

    format!(
        r#"<details class="case">
 <summary><span class="idx">{idx}</span><span class="cname">{name}</span>
  <span class="badge" style="color:{color};background:{bg}">{label}</span>
  <span class="meta">{meta}</span></summary>
 <div class="cdetail"><div class="cid">用例 {case_id}</div>{detail}</div>
</details>"#,
    )
}

/// 由计划名 + 统计 + 逐用例明细生成自包含 HTML 报告。
pub fn report_html(name: &str, stats: &PlanStatistics, cases: &[PlanCase]) -> String {
    let name = escape(name);
    let pass_pct = stats.pass_rate * 100.0;
    let exec_pct = stats.execute_rate * 100.0;
    let (verdict, vcolor) =
        if stats.is_pass { ("通过", "#2e7d32") } else { ("未通过", "#c62828") };

    // 逐用例聚合:报告总耗时 + 断言通过率 + 各状态计数。
    let total_latency: u64 = cases.iter().filter_map(|c| c.result.as_ref()).map(|r| r.latency_ms).sum();
    let (mut assert_total, mut assert_pass) = (0u64, 0u64);
    let (mut n_succ, mut n_err, mut n_fake, mut n_block, mut n_pend) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for c in cases {
        match c.status {
            CaseStatus::Success => n_succ += 1,
            CaseStatus::Error => n_err += 1,
            CaseStatus::FakeError => n_fake += 1,
            CaseStatus::Block => n_block += 1,
            CaseStatus::Pending => n_pend += 1,
        }
        if let Some(r) = &c.result {
            assert_total += r.assertions.len() as u64;
            assert_pass += r.assertions.iter().filter(|a| a.passed).count() as u64;
        }
    }
    let assert_pct = if assert_total == 0 { 100.0 } else { assert_pass as f64 / assert_total as f64 * 100.0 };
    let pct = |n: u64| if stats.total == 0 { 0.0 } else { n as f64 / stats.total as f64 * 100.0 };

    let cases_html: String =
        cases.iter().enumerate().map(|(i, c)| render_case(i + 1, c)).collect();
    let detail_section = if cases.is_empty() {
        "<div class=\"empty\">该计划暂无挂入的用例</div>".to_string()
    } else {
        cases_html
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="zh"><head><meta charset="utf-8"><title>测试计划报告 · {name}</title>
<style>
 body{{font-family:-apple-system,Segoe UI,Helvetica,sans-serif;max-width:960px;margin:32px auto;color:#1f2329;padding:0 16px}}
 h1{{font-size:22px;margin:0 0 2px}}
 .sub{{color:#8a9099;margin-bottom:20px;font-size:13px}}
 .cards{{display:flex;gap:16px;flex-wrap:wrap;margin-bottom:24px}}
 .card{{flex:1;min-width:260px;border:1px solid #eceff1;border-radius:10px;padding:16px 18px;background:#fff}}
 .card h3{{margin:0 0 12px;font-size:14px;color:#5b6470}}
 .row{{display:flex;justify-content:space-between;padding:6px 0;font-size:14px}}
 .row b{{font-weight:600}}
 .verdict{{font-size:18px;font-weight:700;color:{vcolor}}}
 .detail h2{{font-size:16px;margin:24px 0 12px;border-left:3px solid #1664ff;padding-left:8px}}
 details.case{{border:1px solid #eceff1;border-radius:8px;margin:8px 0;background:#fff}}
 summary{{list-style:none;cursor:pointer;display:flex;align-items:center;gap:12px;padding:12px 14px}}
 summary::-webkit-details-marker{{display:none}}
 .idx{{color:#9aa0a6;font-size:12px;min-width:20px}}
 .cname{{font-weight:600;flex:1}}
 .badge{{font-size:12px;font-weight:600;border-radius:4px;padding:2px 8px}}
 .meta{{color:#8a9099;font-size:12px}}
 .cdetail{{padding:4px 16px 16px;border-top:1px solid #f2f3f5}}
 .cid{{color:#b0b6bd;font-size:12px;margin:8px 0}}
 .sec{{font-weight:600;font-size:13px;margin:12px 0 6px;color:#5b6470}}
 table.assert{{width:100%;border-collapse:collapse;font-size:13px}}
 table.assert th,table.assert td{{border:1px solid #eceff1;padding:7px 10px;text-align:left}}
 table.assert th{{background:#fafbfc;color:#5b6470;font-weight:600}}
 pre{{background:#0f1419;color:#d6deeb;padding:12px;border-radius:6px;overflow:auto;font-size:12px;line-height:1.5}}
 .empty{{color:#9aa0a6;font-size:13px;padding:8px 0}}
 details.step{{border:1px solid #f0f2f5;border-radius:6px;margin:6px 0 6px 12px;background:#fafbfc}}
 details.step>summary{{padding:9px 12px;gap:10px}}
 .kind{{font-size:11px;color:#1664ff;background:#eaf1ff;border-radius:4px;padding:2px 7px}}
 .sname{{font-weight:500;flex:1}}
 .stepbody{{padding:2px 12px 12px}}
</style></head>
<body>
 <h1>{name}</h1>
 <div class="sub">状态:{status}　·　结论:<span class="verdict">{verdict}</span></div>
 <div class="cards">
  <div class="card"><h3>报告分析</h3>
   <div class="row"><span>用例总数</span><b>{total}</b></div>
   <div class="row"><span>报告总耗时</span><b>{total_latency} ms</b></div>
   <div class="row"><span>执行率</span><b>{exec_pct:.1}%</b></div>
   <div class="row"><span>通过率</span><b>{pass_pct:.1}%</b></div>
   <div class="row"><span>断言通过率</span><b>{assert_pct:.1}% ({assert_pass}/{assert_total})</b></div>
  </div>
  <div class="card"><h3>用例状态分布</h3>
   <div class="row"><span style="color:#2e7d32">● 通过</span><b>{n_succ}　{p_succ:.1}%</b></div>
   <div class="row"><span style="color:#c62828">● 失败</span><b>{n_err}　{p_err:.1}%</b></div>
   <div class="row"><span style="color:#ef6c00">● 误报</span><b>{n_fake}　{p_fake:.1}%</b></div>
   <div class="row"><span style="color:#455a64">● 阻塞</span><b>{n_block}　{p_block:.1}%</b></div>
   <div class="row"><span style="color:#757575">● 未执行</span><b>{n_pend}　{p_pend:.1}%</b></div>
  </div>
 </div>
 <div class="detail"><h2>报告明细</h2>{detail_section}</div>
</body></html>"#,
        name = name,
        status = stats.status.as_str(),
        verdict = verdict,
        vcolor = vcolor,
        total = stats.total,
        total_latency = total_latency,
        exec_pct = exec_pct,
        pass_pct = pass_pct,
        assert_pct = assert_pct,
        assert_pass = assert_pass,
        assert_total = assert_total,
        n_succ = n_succ, p_succ = pct(n_succ),
        n_err = n_err, p_err = pct(n_err),
        n_fake = n_fake, p_fake = pct(n_fake),
        n_block = n_block, p_block = pct(n_block),
        n_pend = n_pend, p_pend = pct(n_pend),
        detail_section = detail_section,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AssertionResult, CaseResult, ExecStatus};

    fn stats() -> PlanStatistics {
        PlanStatistics {
            status: ExecStatus::Completed,
            total: 2,
            pass_rate: 0.5,
            execute_rate: 1.0,
            is_pass: false,
        }
    }

    fn cases() -> Vec<PlanCase> {
        vec![
            PlanCase {
                case_id: "c1".into(),
                name: "健康检查".into(),
                status: CaseStatus::Success,
                result: Some(CaseResult {
                    latency_ms: 12,
                    response_size: 3,
                    status_code: Some(200),
                    assertions: vec![AssertionResult {
                        item: "状态码".into(),
                        actual: "200".into(),
                        condition: "等于".into(),
                        expected: "200".into(),
                        passed: true,
                        reason: String::new(),
                    }],
                    body: Some("ok".into()),
                    ..Default::default()
                }),
            },
            PlanCase {
                case_id: "c2".into(),
                name: "失败用例".into(),
                status: CaseStatus::Error,
                result: Some(CaseResult {
                    latency_ms: 8,
                    response_size: 5,
                    status_code: Some(200),
                    assertions: vec![AssertionResult {
                        item: "状态码".into(),
                        actual: "200".into(),
                        condition: "等于".into(),
                        expected: "500".into(),
                        passed: false,
                        reason: "期望 500,实际 200".into(),
                    }],
                    body: None,
                    ..Default::default()
                }),
            },
        ]
    }

    fn scenario_case() -> PlanCase {
        PlanCase {
            case_id: "scn1".into(),
            name: "下单场景".into(),
            status: CaseStatus::Success,
            result: Some(CaseResult {
                latency_ms: 30,
                steps: vec![
                    StepResult {
                        name: "登录".into(),
                        kind: "接口用例".into(),
                        status: CaseStatus::Success,
                        latency_ms: 10,
                        status_code: Some(200),
                        assertions: vec![AssertionResult {
                            item: "状态码".into(),
                            actual: "200".into(),
                            condition: "等于".into(),
                            expected: "200".into(),
                            passed: true,
                            reason: String::new(),
                        }],
                        children: vec![],
                    },
                    StepResult {
                        name: "循环下单".into(),
                        kind: "循环控制器".into(),
                        status: CaseStatus::Success,
                        latency_ms: 20,
                        status_code: None,
                        assertions: vec![],
                        children: vec![StepResult {
                            name: "创建订单".into(),
                            kind: "接口用例".into(),
                            status: CaseStatus::Success,
                            latency_ms: 20,
                            status_code: Some(201),
                            assertions: vec![],
                            children: vec![],
                        }],
                    },
                ],
                ..Default::default()
            }),
        }
    }

    #[test]
    fn renders_scenario_step_tree() {
        let html = report_html("场景", &stats(), &[scenario_case()]);
        assert!(html.contains("步骤"));
        assert!(html.contains("登录"));
        assert!(html.contains("循环控制器"));
        assert!(html.contains("创建订单")); // 嵌套子步骤
        assert!(html.contains("接口用例"));
    }

    #[test]
    fn renders_headers_and_request_panels() {
        use crate::domain::RequestInfo;
        let mut cs = cases();
        if let Some(r) = cs[0].result.as_mut() {
            r.response_headers = vec![("Content-Type".into(), "application/json".into())];
            r.request = Some(RequestInfo {
                method: "GET".into(),
                url: "http://x/healthz".into(),
                headers: vec![("Accept".into(), "*/*".into())],
                body: None,
            });
        }
        let html = report_html("面板", &stats(), &cs);
        assert!(html.contains("响应头"));
        assert!(html.contains("Content-Type"));
        assert!(html.contains("实际请求"));
        assert!(html.contains("http://x/healthz"));
    }

    #[test]
    fn renders_summary_and_per_case_detail() {
        let html = report_html("冒烟", &stats(), &cases());
        assert!(html.contains("冒烟"));
        assert!(html.contains("报告明细"));
        // 逐用例
        assert!(html.contains("健康检查"));
        assert!(html.contains("失败用例"));
        // 断言表项
        assert!(html.contains("断言项"));
        assert!(html.contains("期望 500,实际 200"));
        // 报告总耗时 = 12+8
        assert!(html.contains("20 ms"));
        // 断言通过率 1/2
        assert!(html.contains("(1/2)"));
        // 结论未通过
        assert!(html.contains("未通过"));
    }

    #[test]
    fn empty_plan_shows_placeholder() {
        let html = report_html("空计划", &stats(), &[]);
        assert!(html.contains("该计划暂无挂入的用例"));
    }

    #[test]
    fn escapes_content() {
        let mut cs = cases();
        cs[0].name = "<script>".into();
        let html = report_html("<x>", &stats(), &cs);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
