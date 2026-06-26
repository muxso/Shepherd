//! sqlx `migrate!` 按文件名首段数字作版本号,同版本号多文件会被静默去重(丢迁移 → 缺列 → 运行期 500)。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn version_of(name: &str) -> Option<u64> {
    if !name.ends_with(".sql") {
        return None;
    }
    name.split('_').next()?.parse().ok()
}

fn sql_files() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".sql"))
        .collect()
}

#[test]
fn migration_versions_are_unique() {
    let mut by_version: HashMap<u64, Vec<String>> = HashMap::new();
    for name in sql_files() {
        let v = version_of(&name)
            .unwrap_or_else(|| panic!("迁移文件缺少数字版本前缀(NNNN_…):{name}"));
        by_version.entry(v).or_default().push(name);
    }
    let mut dups: Vec<_> = by_version
        .iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(v, files)| format!("版本 {v}: {files:?}"))
        .collect();
    dups.sort();
    assert!(
        dups.is_empty(),
        "迁移版本号重复 —— sqlx 会静默丢失其中的迁移!请重编号:\n{}",
        dups.join("\n")
    );
}

#[test]
fn migration_names_well_formed() {
    let bad: Vec<String> = sql_files().into_iter().filter(|n| version_of(n).is_none()).collect();
    assert!(bad.is_empty(), "迁移文件名应为 `NNNN_描述.sql`:{bad:?}");
}

#[test]
fn version_parsing() {
    assert_eq!(version_of("0073_bug_created_by.sql"), Some(73));
    assert_eq!(version_of("0001_init.sql"), Some(1));
    assert_eq!(version_of("readme.md"), None);
    assert_eq!(version_of("abc_x.sql"), None);
}
