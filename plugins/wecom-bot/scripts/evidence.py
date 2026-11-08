"""Render collected activity evidence without model-generated dates or claims."""
import datetime as dt
import re
from urllib.parse import urlsplit

SHANGHAI = dt.timezone(dt.timedelta(hours=8))


def evidence_request(question):
    value = question.lower()
    if any(word in value for word in ("统计图", "柱状图", "饼图", "热力图")):
        return False
    if any(word in value for word in ("如何实现", "怎么实现", "实现原理", "调用链")):
        return False
    source = any(word in value for word in ("git", "代码", "提交", "云效"))
    period = any(word in value for word in ("本周", "上周", "这周", "今天", "昨天", "本月", "上月")) or bool(re.search(r"(?:最近|近)\s*\d+\s*天|\d{4}-\d{2}-\d{2}", value))
    work = any(word in value for word in ("梳理", "核对", "核实", "真实性", "干啥", "干了", "做了", "做什么", "干什么", "总结", "汇总", "哪些", "对应", "工作", "摸鱼", "偷懒", "干活", "贡献", "有没有提交", "有无提交"))
    return source and (work or (period and any(word in value for word in ("分析", "查看", "检查", "评价"))))


def activity_window(question, now=None):
    today = (now or dt.datetime.now(SHANGHAI)).astimezone(SHANGHAI).date()
    dates = re.findall(r"\d{4}-\d{2}-\d{2}", question)
    if dates:
        start = dt.date.fromisoformat(dates[0])
        end = dt.date.fromisoformat(dates[1]) if len(dates) > 1 else start
    elif "上周" in question:
        end = today-dt.timedelta(days=today.weekday()+1)
        start = end-dt.timedelta(days=6)
    elif "本周" in question or "这周" in question:
        start, end = today-dt.timedelta(days=today.weekday()), today
    elif "昨天" in question:
        start = end = today-dt.timedelta(days=1)
    elif "今天" in question:
        start = end = today
    elif "本月" in question:
        start, end = today.replace(day=1), today
    else:
        match = re.search(r"(?:最近|近)\s*(\d+)\s*天", question)
        days = int(match.group(1)) if match else 7
        start, end = today-dt.timedelta(days=days-1), today
    if start > end or end > today or not 1 <= (today-start).days+1 <= 365:
        raise ValueError("invalid_activity_window")
    return start.isoformat(), end.isoformat(), (today-start).days+1


def in_window(event, start, end):
    return start <= str(event.get("date") or "") <= end


def text(value, limit=160):
    return re.sub(r"[\r\n\[\]`<>]", " ", str(value or "")).strip()[:limit]


def link(label, url):
    label = text(label)
    if isinstance(url, str) and urlsplit(url).scheme in ("https", "http") and not urlsplit(url).username and not urlsplit(url).password and not re.search(r"[\s()<>]", url):
        return f"[{label}]({url})"
    return label


def selected_people(question, data):
    names = {str(m.get("name") or "") for m in data.get("members", [])}
    for event in data.get("events", []):
        names.add(str(event.get("actor") or ""))
        if event.get("authorRaw") and event["authorRaw"].lower() in question.lower():
            names.add(event.get("actor") or "")
    matched = {name for name in names if len(name) >= 2 and name.lower() in question.lower()}
    matched.update(event["actor"] for event in data.get("events", []) if event.get("authorRaw") and str(event["authorRaw"]).lower() in question.lower() and event.get("actor"))
    return matched


def scoped_events(question, data, start, end):
    people = selected_people(question, data)
    events = [e for e in data.get("events", []) if e.get("kind") == "commit" and in_window(e, start, end) and (not people or e.get("actor") in people)]
    return people, sorted(events, key=lambda e: str(e.get("at") or ""), reverse=True)


def scope_header(data, start, end, people):
    scope = "、".join(sorted(people)) if people else "项目全部已采集成员"
    lines = [f"**核对范围：{start} ～ {end}（上海时间，含首尾）**", f"人员：{scope}。同步截至：{text(data.get('observedAt') or '未知')}。"]
    if data.get("coverage", {}).get("complete") is False:
        lines.append("当前采集覆盖不完整，以下仅代表已同步记录。")
    return lines


def activity_summary(question, data, start, end):
    people, events = scoped_events(question, data, start, end)
    lines = scope_header(data, start, end, people)
    if any(word in question for word in ("摸鱼", "偷懒", "绩效", "工作表现")):
        lines.append("Git 记录只能说明已观测代码活动，不能据此判断是否摸鱼、工作时长或绩效。")
    if not events:
        return "\n\n".join(lines+["该范围未找到已采集的 Git 提交。不能据此认定没有工作；请检查账号归并、分支采集与同步覆盖。"])
    repos = {str(e.get("repoId") or e.get("repo") or "未标识仓库") for e in events}
    requirements = {r["externalId"] for e in events for r in e.get("requirements", []) if r.get("externalId")}
    unknown_lines = sum(e.get("additions") is None or e.get("deletions") is None for e in events)
    additions = sum(e.get("additions") or 0 for e in events)
    deletions = sum(e.get("deletions") or 0 for e in events)
    lines += [f"已采集 **{len(events)} 次提交 · {len(repos)} 个仓库 · 关联 {len(requirements)} 项需求**。新增 {additions:,} / 删除 {deletions:,} 行。"+(f"另有 {unknown_lines} 次提交缺少完整行数，以上为已知部分。" if unknown_lines else ""), "**提交依据（最近 15 条）**"]
    for event in events[:15]:
        reference = link(str(event.get("sha") or event.get("externalId") or "提交")[:12], event.get("url"))
        related = "；".join(link(r.get("serial") or r.get("title") or r.get("externalId"), r.get("url")) for r in event.get("requirements", [])[:3]) or "未关联需求"
        lines.append(f"- {text(event.get('date'))} · {text(event.get('actor'))} · {text(event.get('repo') or event.get('repoId'))}\n  {text(event.get('title'), 140)} · {reference}\n  {related}")
    if len(events) > 15:
        lines.append(f"其余 {len(events)-15} 次提交请在 Shepherd 团队产出明细中查看。")
    lines.append("依据为已同步 Git 提交及其云效需求关联；提交、需求状态和实际发布是不同证据，不能推断工时或日报真实性。")
    return "\n\n".join(lines)


def report_items(question, history):
    candidates = [question]+[content for role, content in reversed(history) if role == "user"]
    for candidate in candidates:
        if not re.search(r"[【\[][^】\]]+[】\]]", candidate) and len(candidate.splitlines()) < 4:
            continue
        rows = []
        for raw in candidate.splitlines():
            row = re.sub(r"^\s*(?:[-*•]|\d+[.)、])\s*", "", raw).strip()
            if len(row) < 8 or re.match(r"^[一二三四五六七八九十]+[、.]", row):
                continue
            if any(term in row for term in ("从git记录", "逐条核对", "日报真实性", "对应的代码")):
                continue
            rows.append(row)
        if rows:
            return list(dict.fromkeys(rows))[:40]
    return []


def grams(value):
    value = re.sub(r"[【\[][^】\]]+[】\]]", "", value.lower())
    words = set(re.findall(r"[a-z][a-z0-9_-]{2,}", value))
    for run in re.findall(r"[\u4e00-\u9fff]+", value):
        words.update(run[i:i+2] for i in range(len(run)-1))
    return words-{"新增", "优化", "完成", "进行", "测试", "问题", "修复", "支持", "开发", "调整", "功能", "同步", "需求", "已发", "发布"}


def report_evidence(question, history, data, start, end):
    people, events = scoped_events(question, data, start, end)
    lines = scope_header(data, start, end, people)
    items = report_items(question, history)
    if not items:
        return "\n\n".join(lines+["请将要核对的日报条目以文字发给我，并注明人员和日期。当前无法读取未发送给机器人的群历史或附件。"])
    references = {}
    for event in events:
        for requirement in event.get("requirements", []):
            key = requirement.get("externalId")
            if key:
                references.setdefault(key, {"requirement": requirement, "events": []})["events"].append(event)
    lines += [f"已读取所选范围的 {len(events)} 次 Git 提交及 {len(references)} 项云效关联需求。以下为候选证据定位，不是对日报真假的自动判定。", "标题相似只作候选；发布、测试次数、协作和进度百分比需要对应发布或测试记录，不能由提交量证明。"]
    for index, item in enumerate(items, 1):
        tokens = grams(item)
        candidates = []
        for ref in references.values():
            req = ref["requirement"]
            serial = str(req.get("serial") or "")
            target = grams(str(req.get("title") or ""))
            shared = tokens & target
            exact = bool(serial and serial.lower() in item.lower())
            if exact or (len(shared) >= 3 and len(shared)/max(1, len(tokens | target)) >= 0.12):
                candidates.append((100 if exact else len(shared)/max(1, len(tokens | target)), ref))
        candidates.sort(key=lambda row: -row[0])
        lines.append(f"**{index}. {text(item, 160)}**")
        if not candidates:
            lines.append("未找到明确的关联提交候选。请补充需求编号或提交 SHA；缺少采集证据不等于条目不真实。")
            continue
        for score, ref in candidates[:1]:
            req = ref["requirement"]; commits = ref["events"]
            req_label = (str(req.get("serial") or "")+" "+str(req.get("title") or req.get("externalId"))).strip()
            lines.append("候选需求："+link(req_label, req.get("url")))
            evidence = commits[0]
            lines.append(f"范围内关联 {len(commits)} 次提交；示例 {text(evidence.get('repo') or evidence.get('repoId'))} · "+link(str(evidence.get("sha") or evidence.get("externalId"))[:12], evidence.get("url")))
        lines.append("需确认是否为同一业务条目；发布与测试完成情况尚未核验。")
    return "\n\n".join(lines)
