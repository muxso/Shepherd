use std::fs;
use std::path::Path;
const FORBIDDEN: &[&str] =
    &["sqlx", "axum", "reqwest", "tower_http", "tonic", "redis", "tokio_tungstenite"];
#[test]
fn pure_layers_have_no_io_imports() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    for layer in ["src/domain", "src/ports"] {
        let dir = base.join(layer);
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
                assert!(!src.contains(&pat), "{} 含 `{pat}`(IO 只能在 plugins)", path.display());
            }
        }
    }
}
