use std::fs;
use std::path::Path;
const FORBIDDEN: &[&str] = &["sqlx", "axum", "reqwest", "tower_http"];
#[test]
fn storage_has_no_native_business_table_dependencies() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in ["src/adapters/pg.rs", "migrations/0001_external_facts.sql"] {
        let source = fs::read_to_string(base.join(relative)).expect("storage source");
        for table in [
            "ms_project",
            "ms_requirement",
            "ms_functional_case",
            "ms_api_module",
            "ms_task",
            "ms_delivery",
        ] {
            assert!(!source.contains(table), "{relative} depends on native table {table}");
        }
    }
}
#[test]
fn pure_layers_have_no_io_imports() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    for layer in ["src/domain", "src/ports", "src/application"] {
        let dir = base.join(layer);
        // Some crates have no domain layer (e.g. pure use-case orchestrators); only check layers that exist.
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
                assert!(
                    !src.contains(&pat),
                    "{} contains `{pat}` (IO belongs only in adapters)",
                    path.display()
                );
            }
        }
    }
}
