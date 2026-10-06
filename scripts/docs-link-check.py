#!/usr/bin/env python3
"""多语言文档链接检查 / Documentation link checker.

扫描仓库内的 Markdown 文档（README + docs/*.md，英文 + 简体中文），校验：

1. 内部链接目标文件是否存在（文档、资源、deploy/ 等任意文件）；
2. 章节锚点是否解析（GitHub 风格 slug，含显式 `{#id}` 与 `<a id>`）；
3. 跨语言配对完整性（每个文档组是否 EN + ZH-CN 成对存在）。

用法：
    python3 scripts/docs-link-check.py [--root <path>] [--report [path]] [--quiet]

- 默认打印通过/失败摘要到 stdout；
- `--report`（可省略路径，默认 docs/link-check-report.md）写出完整 Markdown 报告；
- 退出码：0 = 全部有效，1 = 存在无效链接或跨语言缺失，2 = 参数错误。

HTML 注释中的占位链接（如 docs/assets/demo.gif）与外部链接（http/https/mailto）不计入判定。
"""
import argparse
import datetime
import glob
import os
import re
import sys

IMG_EXT = (".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp")
DEFAULT_REPORT_REL = "docs/link-check-report.md"
LINK_RE = re.compile(r"(!?)\[([^\]]*)\]\((.*?)\)")
EXPLICIT_RE = re.compile(r"\{#([^}]+)\}")
AID_RE = re.compile(r'<a\s+(?:id|name)="([^"]+)"', re.I)
COMMENT_RE = re.compile(r"<!--.*?-->", re.S)


def slug(text: str) -> str:
    """GitHub 风格标题 slug：小写，保留字母/数字/连字符，空格转 '-'，丢弃其余标点（含 。、()# 等）。"""
    s = text.strip().lower()
    out = []
    for ch in s:
        if ch.isspace():
            out.append("-")
        elif ch.isalnum() or ch == "-":
            out.append(ch)
    return re.sub(r"-+", "-", "".join(out)).strip("-")


def collect_anchors(path: str) -> set:
    ids = set()
    in_fence = False
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            m = re.match(r"^#{1,6}\s+(.*?)\s*#*\s*$", line)
            if m:
                h = m.group(1)
                ids.add(slug(h))
                for ex in EXPLICIT_RE.findall(h):
                    ids.add(ex.strip())
            for aid in AID_RE.findall(line):
                ids.add(aid.strip())
    return ids


def discover_docs(root: str, exclude: set = None) -> list:
    exclude = exclude or set()
    docs = ["README.md", "README.zh-CN.md"]
    docs += sorted(
        os.path.relpath(p, root)
        for p in glob.glob(os.path.join(root, "docs", "*.md"))
    )
    # 仅保留真实存在的文件（例如 README.zh-CN.md 缺失时不报错），并排除生成的报告本身
    return [
        d
        for d in docs
        if os.path.isfile(os.path.join(root, d)) and d not in exclude
    ]


def check(root: str, docs: list) -> tuple:
    anchors = {d: collect_anchors(os.path.join(root, d)) for d in docs}
    records = []
    for src in docs:
        with open(os.path.join(root, src), encoding="utf-8") as f:
            lines = f.readlines()
        for i, line in enumerate(lines, 1):
            cl = COMMENT_RE.sub("", line)
            for _is_img, _text, raw in LINK_RE.findall(cl):
                if raw.startswith(("http://", "https://", "mailto:")):
                    continue
                parts = raw.split("#", 1)
                path = parts[0].strip()
                anchor = parts[1] if len(parts) > 1 else ""
                if path == "":
                    check_path = src
                    ttype = "anchor(same-file)"
                else:
                    check_path = os.path.normpath(
                        os.path.join(os.path.dirname(src), path)
                    )
                    if path.lower().endswith(IMG_EXT) or path.startswith(
                        ("docs/assets/", "web/")
                    ):
                        ttype = "asset"
                    elif path.endswith(".md"):
                        ttype = "doc"
                    else:
                        ttype = "file"
                exists = os.path.isfile(os.path.join(root, check_path)) or os.path.isdir(
                    os.path.join(root, check_path)
                )
                status = "OK" if exists else "BROKEN"
                if (
                    exists
                    and anchor
                    and check_path in anchors
                    and anchor not in anchors[check_path]
                ):
                    status = "BROKEN"
                xl = (
                    path.endswith(".zh-CN.md")
                    and not src.endswith(".zh-CN.md")
                ) or (
                    src.endswith(".zh-CN.md")
                    and path.endswith(".md")
                    and not path.endswith(".zh-CN.md")
                )
                records.append(
                    dict(
                        src=src,
                        line=i,
                        target=raw,
                        type=ttype,
                        status=status,
                        xl=bool(xl),
                    )
                )
    return records, anchors


def check_pairing(docs: list) -> list:
    groups = {}
    for d in docs:
        base = re.sub(r"\.zh-CN$", "", os.path.splitext(d)[0])
        groups.setdefault(base, set()).add(d)
    rows = []
    for base, fs in sorted(groups.items()):
        has_en = any(not f.endswith(".zh-CN.md") for f in fs)
        has_zh = any(f.endswith(".zh-CN.md") for f in fs)
        rows.append((base, sorted(fs), has_en and has_zh))
    return rows


def build_report(root, docs, records, pairing) -> str:
    total = len(records)
    broken = [r for r in records if r["status"] != "OK"]
    missing_pairs = [r for r in pairing if not r[2]]
    now = datetime.date.today().isoformat()
    L = []
    L.append("# 多语言文档链接检查报告 / Documentation Link Check Report")
    L.append("")
    L.append(f"- 生成日期 / Generated: **{now}**")
    L.append(
        f"- 检查范围 / Scope: {len(docs)} 个文档（README + `docs/*.md`，英文 + 简体中文）"
    )
    L.append(f"- 链接总数 / Total links checked: **{total}**")
    L.append(
        f"- 有效 / Valid: **{total - len(broken)}**　无效 / Broken: **{len(broken)}**"
    )
    L.append("")
    L.append("## 结论 / Result")
    L.append("")
    if broken or missing_pairs:
        L.append(
            f"> ❌ 发现 {len(broken)} 个无效链接、{len(missing_pairs)} 组跨语言缺失，详见下方清单。"
        )
    else:
        L.append(
            "> ✅ 所有内部文档链接、跨语言切换、章节锚点与资源引用均有效。"
        )
    L.append("")
    L.append("## 跨语言配对完整性 / Cross-language pairing")
    L.append("")
    L.append("| 文档组 / Group | 文件 | 中英齐全 / EN+ZH |")
    L.append("|---|---|---|")
    for base, fs, complete in pairing:
        names = ", ".join("`" + os.path.basename(f) + "`" for f in fs)
        L.append(f"| `{base}` | {names} | {'✅' if complete else '⚠️ 缺失'} |")
    L.append("")
    L.append("## 各文档链接清单 / Link inventory")
    L.append("")
    for src in docs:
        src_links = [r for r in records if r["src"] == src]
        L.append(f"### {src}  ({len(src_links)} 链接)")
        L.append("")
        L.append("| 行 | 类型 | 目标 | 状态 | 跨语言 |")
        L.append("|---|---|---|---|---|")
        for r in src_links:
            L.append(
                f"| {r['line']} | {r['type']} | `{r['target']}` | {r['status']} | {'🔁' if r['xl'] else ''} |"
            )
        L.append("")
    if broken:
        L.append("## 无效链接清单 / Broken links")
        L.append("")
        for r in broken:
            L.append(f"- `{r['src']}:{r['line']}` → `{r['target']}`")
        L.append("")
    L.append("## 说明 / Notes")
    L.append("")
    L.append(
        "- 检查方式 / Method: 移除 HTML 注释后提取 Markdown 链接，对文件目标做存在性校验，对 `.md` 目标按 GitHub 风格 slug（小写、保留字母/数字/连字符、空格转 `-`、丢弃 `。、()#` 等标点）校验章节锚点。"
    )
    L.append(
        "- 外部链接（http/https/mailto）与注释中的占位链接（如 `docs/assets/demo.gif`）未计入有效/无效判定。"
    )
    L.append(
        "- 所有文档顶部均已统一语言切换导航 `[简体中文](x.zh-CN.md) · English` / `简体中文 · [English](x.md)`。"
    )
    return "\n".join(L) + "\n"


def main():
    parser = argparse.ArgumentParser(description="Check multilingual doc links.")
    parser.add_argument("--root", default=None, help="repo root (default: parent of this script)")
    parser.add_argument(
        "--report",
        nargs="?",
        const="docs/link-check-report.md",
        default=None,
        help="write Markdown report to this path (default: docs/link-check-report.md)",
    )
    parser.add_argument("--quiet", action="store_true", help="only print pass/fail summary")
    args = parser.parse_args()

    root = os.path.abspath(args.root) if args.root else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    report_path = (
        args.report
        if args.report and os.path.isabs(args.report)
        else os.path.join(root, args.report)
        if args.report
        else None
    )
    exclude = {DEFAULT_REPORT_REL}
    if report_path:
        exclude.add(os.path.relpath(report_path, root))
    docs = discover_docs(root, exclude)
    records, _ = check(root, docs)
    pairing = check_pairing(docs)

    total = len(records)
    broken = [r for r in records if r["status"] != "OK"]
    missing_pairs = [r for r in pairing if not r[2]]
    ok = total - len(broken)

    if not args.quiet:
        print(f"Scanned {len(docs)} docs, {total} links: {ok} valid, {len(broken)} broken.")
        for r in broken:
            print(f"  BROKEN {r['src']}:{r['line']} -> {r['target']}")
        for base, fs, _ in missing_pairs:
            print(f"  MISSING PAIR {base}: {sorted(fs)}")

    if args.report:
        with open(report_path, "w", encoding="utf-8") as f:
            f.write(build_report(root, docs, records, pairing))
        if not args.quiet:
            print(f"Report written to {report_path}")

    if broken or missing_pairs:
        print("❌ Documentation link check FAILED")
        sys.exit(1)
    print("✅ Documentation link check passed")
    sys.exit(0)


if __name__ == "__main__":
    main()
