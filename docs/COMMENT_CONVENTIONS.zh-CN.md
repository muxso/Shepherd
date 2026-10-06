# 注释规范

简体中文 · [English](COMMENT_CONVENTIONS.md)

本仓库注释的统一规则。目标是让每个 crate 的注释都保持一致、用英文、对 rustdoc 友好。

## 1. 注释形式

| 语法 | 用途 |
| ----------- | --------------------------------------------------- |
| `//!` | 模块级或 crate 级文档(放在文件顶部)。 |
| `///` | 紧跟其后的条目的文档。 |
| `//` | 行内说明、`TODO`、理由——非公开 API。 |
| `// TODO:` | 待办工作(英文)。优先附上 issue 链接。 |

- 凡是公开面的一部分(导出的类型、函数、方法、模块),用 `///` / `//!`;内部推理用 `//`。
- 文档文本不要用 `/* ... */` 块注释;正文用行注释,这样 `cargo doc` 才能收录。行内 `/* */` 用于临时注释代码没问题。

## 2. 语言

- 所有**文档文本**都用**英文**:`//`、`///`、`//!` 注释行,`utoipa` 的 `description = "..."` 属性,以及 `#[error(...)]` / `#[ignore = "..."]` / `#[schema(...)]` 里的消息。
- 不要用中文、拼音或中英混杂写文档。
- **例外:** 作为测试夹具或 mock 数据的字符串字面量(例如 `#[test]` / `r#"..."#` 里的 `"登录"`、`"张三"`)。这些是有意用来跑 i18n 的,保持原样。

## 3. 风格

- **首字母大写。** 每句以句号结尾(`.`)。
- `//`、`///`、`//!` 后**空一格**:`/// Returns the count.`
- 用**第三人称或祈使句**作为摘要句:
  - `/// Returns the number of active users.`
  - `/// Parses the raw payload into a `Proposal`.`
  - 避免 `This function returns...`;以动作开头。
- 注释**精简**;不要复述代码一眼就看得懂的东西。解释 *为什么*、约束,以及不直观的行为。
- 结构体 / 枚举字段,优先用以句号结尾的名词短语:`/// The user's display name.`

## 4. 代码与标识符

- 类型、函数、模块、路径用反引号包起来:`Vec<T>`、`parse()`、`crate::domain::Proposal`、`Result<T, Error>`。
- 超过一行的行内代码片段,用围栏代码块:

  ```rust
  let req = Proposal::new(..)?;
  ```

## 5. Markdown

- 文档注释(`///`、`//!`)支持 Markdown:列表、链接、表格、代码围栏。
- 列表用 `-`;仅当列表需要引导句时,在首项前空一行。

## 6. 示例

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

## 7. 检查

确认注释行里没有残留中文:

```bash
# 粗检:标出 // 行里、字符串字面量之外的中文字符
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
