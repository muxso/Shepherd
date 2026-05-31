//! 架构守卫:静态扫描,确保 domain/ports/application **不引用任何 IO crate**。
//! feature 开启(pg/http)后 sqlx/axum 在依赖图里可见,纯编译屏障失效——本测试兜底。
//! 默认 `cargo test` 即跑(不依赖 feature)。

use std::fs;
use std::path::Path;

const FORBIDDEN: &[&str] = &["sqlx", "axum", "reqwest", "tower_http"];

#[test]
fn pure_layers_have_no_io_imports() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    for layer in ["src/domain", "src/ports", "src/application"] {
        scan(&base.join(layer));
    }
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
                assert!(
                    !src.contains(&pat),
                    "纯层禁止引用 IO crate:{} 含 `{pat}`(IO 只能在 adapters)",
                    path.display()
                );
            }
        }
    }
}
