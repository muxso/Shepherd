use std::fs;
use std::path::Path;
const FORBIDDEN: &[&str] = &["sqlx", "axum", "reqwest", "tower_http"];
#[test]
fn pure_layers_have_no_io_imports() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    for layer in ["src/domain", "src/ports", "src/application"] {
        let dir = base.join(layer);
        // 部分 crate 无 domain 层(如编排器纯用例编排);存在的层才检查。
        if dir.exists() {
            scan(&dir);
        }
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
                assert!(!src.contains(&pat), "{} 含 `{pat}`(IO 只能在 adapters)", path.display());
            }
        }
    }
}
