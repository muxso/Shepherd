//! 架构守卫:domain 层不得引用任何 IO crate(断言判定是纯函数,HTTP 只在 adapters)。
//!
//! 本 crate 比其它上下文更需要这道源码扫描:`reqwest` 在 `[dependencies]` 里**无条件**引入
//! (非 feature 门控),默认 build 也在依赖图内 —— §2.1 的「默认 build 不启用 IO feature」
//! 编译期屏障对它不成立,纯层不碰 IO 的保证全靠这道兜底扫描。
use std::fs;
use std::path::Path;

const FORBIDDEN: &[&str] = &["sqlx", "axum", "reqwest", "tower_http"];

#[test]
fn pure_layers_have_no_io_imports() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    scan(&base.join("src/domain"));
}

fn scan(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            scan(&path);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = fs::read_to_string(&path).expect("read file");
        for token in FORBIDDEN {
            for pat in [format!("use {token}"), format!("{token}::")] {
                assert!(!src.contains(&pat), "{} 含 `{pat}`(IO 只能在 adapters)", path.display());
            }
        }
    }
}
