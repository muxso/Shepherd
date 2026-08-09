# Comment Conventions

Unified rules for comments in this workspace. The goal is consistent, English,
rustdoc-friendly comments across every crate.

## 1. Comment forms

| Syntax      | Use for                                              |
| ----------- | --------------------------------------------------- |
| `//!`       | Module- or crate-level docs (placed at the top).    |
| `///`       | Documentation for the item that follows it.         |
| `//`        | Inline notes, `TODO`, rationale — not public API.   |
| `// TODO:`  | Pending work (English). Prefer an issue link.       |

- Use `///` / `//!` for anything that is part of the public surface (exported
  types, functions, methods, modules). Use `//` for internal reasoning.
- Never use `/* ... */` block comments for doc text; keep prose as line comments
  so `cargo doc` picks it up. Inline `/* */` is fine for temporarily disabling code.

## 2. Language

- All **documentation text** is written in **English**: `//`, `///`, `//!` comment
  lines, `utoipa` `description = "..."` attributes, and `#[error(...)]` /
  `#[ignore = "..."]` / `#[schema(...)]` messages.
- Do not write documentation in Chinese, Pinyin, or mixed languages.
- **Out of scope:** string literals that are test fixtures or mock data (e.g.
  `"登录"`, `"张三"` in `#[test]` / `r#"..."#` bodies). These intentionally exercise
  i18n and are left as-is.

## 3. Style

- **Capitalize the first word.** End every sentence with a period (`.").
- **One space** after `//`, `///`, `//!`: `/// Returns the count.`
- Use **third-person or imperative** summary sentences:
  - `/// Returns the number of active users.`
  - `/// Parses the raw payload into a `Proposal`.`
  - Avoid `This function returns...`; lead with the action.
- Keep comments **concise**; do not restate what the code obviously does.
  Explain *why*, constraints, and non-obvious behavior.
- For struct/enum fields, prefer a noun phrase ending in a period:
  `/// The user's display name.`

## 4. Code and identifiers

- Wrap types, functions, modules, and paths in backticks:
  `Vec<T>`, `parse()`, `crate::domain::Proposal`, `Result<T, Error>`.
- Wrap inline code snippets in fenced blocks when longer than a line:

  ```rust
  let req = Proposal::new(..)?;
  ```

## 5. Markdown

- Doc comments (`///`, `//!`) accept Markdown: lists, links, tables, code fences.
- For lists, use `-` and a blank line before the first item only when the list
  needs a lead-in sentence.

## 6. Examples

```rust
//! HTTP adapters for the requirement crate.
//! Exposes REST endpoints backed by the PostgreSQL repository.

/// Creates a new requirement from the given draft.
///
/// Returns an error if the title is empty after trimming.
pub fn create(draft: Draft) -> Result<Requirement> {
    // Reject empty titles early to avoid a round-trip to the DB.
    if draft.title.trim().is_empty() {
        return Err(Error::EmptyTitle);
    }
    // ...
}
```

## 7. Checking

To confirm no Chinese remains in comment lines:

```bash
# crude check: flag any CJK outside of string literals in // lines
python3 - <<'PY'
import os, re
def cjk(s): return any('\u4e00' <= c <= '\u9fff' for c in s)
for root, _, fs in os.walk('crates'):
    for f in fs:
        if not f.endswith('.rs'):
            continue
        for i, line in enumerate(open(os.path.join(root, f), encoding='utf-8', errors='ignore'), 1):
            s = re.sub(r'"[^"]*"', '""', line)
            s = re.sub(r"'[^']*'", "''", s)
            if cjk(s) and re.match(r'\s*//', line):
                print(f'{os.path.join(root, f)}:{i}: {line.rstrip()}')
PY
```
