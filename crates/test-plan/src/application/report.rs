//! 测试计划报告渲染(纯函数,零 IO):由统计数据生成自包含 HTML。
//!
//! 渲染逻辑与 IO 解耦,可穷举单测;HTTP 适配器算出 `PlanStatistics` 后调用本函数返回 text/html。

use crate::application::PlanStatistics;

/// HTML 转义(防计划名里的特殊字符破坏页面)。
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 由计划名 + 统计生成自包含 HTML 报告。
pub fn report_html(name: &str, stats: &PlanStatistics) -> String {
    let name = escape(name);
    let pass_pct = stats.pass_rate * 100.0;
    let exec_pct = stats.execute_rate * 100.0;
    let (verdict, color) =
        if stats.is_pass { ("通过", "#2e7d32") } else { ("未通过", "#c62828") };
    format!(
        r#"<!DOCTYPE html>
<html lang="zh"><head><meta charset="utf-8"><title>测试计划报告 · {name}</title>
<style>
 body{{font-family:-apple-system,Segoe UI,sans-serif;max-width:640px;margin:40px auto;color:#222}}
 h1{{font-size:22px;margin:0 0 4px}}
 .status{{color:#666;margin-bottom:24px}}
 table{{width:100%;border-collapse:collapse;margin:16px 0}}
 td,th{{border:1px solid #e0e0e0;padding:10px 14px;text-align:left}}
 th{{background:#fafafa;width:40%}}
 .verdict{{font-size:18px;font-weight:600;color:{color}}}
</style></head>
<body>
 <h1>{name}</h1>
 <div class="status">状态:{status}</div>
 <table>
  <tr><th>用例总数</th><td>{total}</td></tr>
  <tr><th>执行率</th><td>{exec_pct:.1}%</td></tr>
  <tr><th>通过率</th><td>{pass_pct:.1}%</td></tr>
 </table>
 <div class="verdict">结论:{verdict}</div>
</body></html>"#,
        name = name,
        status = stats.status.as_str(),
        total = stats.total,
        exec_pct = exec_pct,
        pass_pct = pass_pct,
        verdict = verdict,
        color = color,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ExecStatus;

    fn stats() -> PlanStatistics {
        PlanStatistics {
            status: ExecStatus::Underway,
            total: 4,
            pass_rate: 0.75,
            execute_rate: 0.5,
            is_pass: true,
        }
    }

    #[test]
    fn renders_name_rates_and_verdict() {
        let html = report_html("冒烟计划", &stats());
        assert!(html.contains("冒烟计划"));
        assert!(html.contains("75.0%")); // 通过率
        assert!(html.contains("50.0%")); // 执行率
        assert!(html.contains("用例总数</th><td>4"));
        assert!(html.contains("结论:通过"));
    }

    #[test]
    fn failing_plan_shows_not_passed() {
        let mut s = stats();
        s.is_pass = false;
        let html = report_html("回归", &s);
        assert!(html.contains("结论:未通过"));
        assert!(html.contains("#c62828")); // 红色
    }

    #[test]
    fn escapes_plan_name() {
        let html = report_html("<script>x</script>", &stats());
        assert!(!html.contains("<script>x"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
