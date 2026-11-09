"""Project-scoped WeCom conversation state and chart rendering."""
import base64
import collections
import hashlib
import hmac
import io
import json
import re
import sqlite3
import time
from pathlib import Path
from urllib.parse import urlsplit, urljoin


def clean_question(value):
    value = re.sub(r"<@[^>]+>", "", str(value or "")).strip()
    return re.sub(r"^@\S+\s+", "", value).strip()[:8000]


def incoming_text(body):
    kind = body.get("msgtype", "text")
    if kind == "text":
        return clean_question((body.get("text") or {}).get("content"))
    if kind == "mixed":
        items = (body.get("mixed") or {}).get("msg_item", [])
        return clean_question("\n".join(str((item.get("text") or {}).get("content") or "") for item in items if item.get("msgtype") == "text"))
    return ""


def citation_footer(citations, public_url=""):
    lines = []
    for index, source in enumerate(citations[:20], 1):
        title = str(source.get("title") or "项目资料").replace("\n", " ")[:120]
        title = title.replace("[", "\\[").replace("]", "\\]")
        url = str(source.get("url") or "")
        if url.startswith("/") and not url.startswith("//"):
            base = urlsplit(public_url)
            url = urljoin(public_url, url) if base.scheme in ("http", "https") and base.netloc and not base.username and not base.password else ""
        parsed = urlsplit(url)
        safe = parsed.scheme in ("http", "https") and parsed.netloc and not parsed.username and not parsed.password and not re.search(r"[\s()<>\\]", url)
        label = f"[{title}]({url})" if safe else title
        date = str(source.get("occurred_at") or "")[:10]
        lines.append(f"[{index}] {label}"+(f" · {date}" if date else ""))
    return "\n\n资料来源：\n"+"\n".join(lines) if lines else ""


def absolute_source_links(text, public_url):
    base = urlsplit(public_url)
    if base.scheme not in ("http", "https") or not base.netloc or base.username or base.password:
        return text
    return re.sub(r"\]\((/[^/\s)<>][^\s)<>]*)\)", lambda m: "]("+urljoin(public_url, m.group(1))+")", text)


def image_reply_caption(answer, image_count):
    """Keep chart replies short while the full answer remains in conversation history."""
    from datetime import datetime, timezone, timedelta
    dates = re.findall(r"\d{4}-\d{2}-\d{2}", answer[:300])
    lines = [f"{dates[0]} ～ {dates[1]}（上海）"] if len(dates) >= 2 else []
    people = re.search(r"人员：([^。\n]+)", answer)
    metric = next((line for line in answer.splitlines() if "查询范围覆盖" in line), "")
    if metric:
        metric = metric.split("，查询范围覆盖", 1)[0].replace("**", "")
        person = people.group(1).replace("项目全部已采集成员", "全员") if people else ""
        lines.append((person+" · " if person else "")+metric)
    else:
        roles = [line.split("同步截至：", 1)[0].strip() for line in answer.splitlines() if any(line.startswith("**"+role+"**") for role in ("研发", "产品", "测试"))]
        if roles:
            lines.extend(roles)
        else:
            first = next((part.strip() for part in answer.split("\n\n") if part.strip()), "统计结果")
            if not lines or first not in lines:
                lines.append(utf8_limit(first, 250))
    observed = re.search(r"同步截至：\s*([0-9T:+.Z-]+)", answer)
    if observed:
        try:
            at = datetime.fromisoformat(observed.group(1).rstrip('.').replace('Z', '+00:00'))
            lines.append("同步："+at.astimezone(timezone(timedelta(hours=8))).strftime("%m-%d %H:%M")+"（上海）")
        except ValueError:
            pass
    lines.append(f"下附 {image_count} 张统计图。仅统计已采集记录。")
    if "图表生成失败" in answer:
        lines.append("部分图片生成失败，请查看来源页。")
    footer = answer.rsplit("资料来源：", 1)[-1] if "资料来源：" in answer else ""
    link = re.search(r"\[[^\]]+\]\((https?://[^\s)<>]+)\)", footer)
    if link:
        lines.append("[查看统计来源]("+link.group(1)+")")
    return utf8_limit("\n".join(lines), 1200)


def utf8_limit(value, limit=19000):
    raw = str(value).encode()
    return raw[:limit].decode(errors="ignore") + ("…" if len(raw) > limit else "")


class Conversations:
    def __init__(self, filename):
        Path(filename).parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.db = sqlite3.connect(filename)
        self.db.executescript("""
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS room(id TEXT PRIMARY KEY,created_at REAL NOT NULL);
        CREATE TABLE IF NOT EXISTS setup(id INTEGER PRIMARY KEY,used INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS received(id TEXT PRIMARY KEY,created_at REAL NOT NULL);
        CREATE TABLE IF NOT EXISTS turn(room TEXT,sender TEXT,role TEXT,content TEXT,created_at REAL);
        """)
        self.db.commit()

    def bind(self, room, supplied, expected):
        if not room or not expected or not hmac.compare_digest(supplied.encode(), expected.encode()):
            return False
        with self.db:
            if self.db.execute("SELECT 1 FROM room WHERE id=?", (room,)).fetchone():
                return True
            if self.db.execute("SELECT 1 FROM setup WHERE used=1").fetchone():
                return False
            self.db.execute("INSERT INTO room VALUES(?,?)", (room, time.time()))
            self.db.execute("INSERT OR REPLACE INTO setup VALUES(1,1)")
        return True

    def allowed(self, room, configured):
        return room in configured or bool(self.db.execute("SELECT 1 FROM room WHERE id=?", (room,)).fetchone())

    def claim(self, message):
        if not message:
            return False
        with self.db:
            self.db.execute("DELETE FROM received WHERE created_at<?", (time.time()-86400,))
            return self.db.execute("INSERT OR IGNORE INTO received VALUES(?,?)", (message, time.time())).rowcount == 1

    def history(self, room, sender):
        rows = self.db.execute("SELECT role,content FROM turn WHERE room=? AND sender=? ORDER BY created_at DESC,rowid DESC LIMIT 12", (room, sender)).fetchall()
        return [list(row) for row in reversed(rows)]

    def append(self, room, sender, question, answer):
        with self.db:
            for role, content in [("user", question), ("assistant", answer)]:
                self.db.execute("INSERT INTO turn VALUES(?,?,?,?,?)", (room, sender, role, utf8_limit(content, 12000), time.time()))
            self.db.execute("DELETE FROM turn WHERE created_at<?", (time.time()-30*86400,))
            self.db.execute("DELETE FROM turn WHERE room=? AND sender=? AND rowid NOT IN (SELECT rowid FROM turn WHERE room=? AND sender=? ORDER BY created_at DESC,rowid DESC LIMIT 100)", (room, sender, room, sender))


def chart_request(question):
    if not (any(word in question.lower() for word in ("产出", "提交", "代码量", "行数", "缺陷", "bug", "仓库", "需求数量")) and any(word in question for word in ("图", "统计", "产出", "情况", "汇总", "数量", "多少", "行数", "代码量", "排行"))):
        return None
    if any(term in question for term in ("昨天", "本周", "上周", "本月", "上月", "今年", "去年")) or re.search(r"\d{1,4}[-/年]\d", question):
        raise ValueError("unsupported_time_range")
    match = re.search(r"(?:最近|近)\s*(\d+)\s*天", question)
    days = int(match.group(1)) if match else (1 if "今天" in question else 7)
    if not 1 <= days <= 365:
        raise ValueError("统计范围需要在 1～365 天内")
    role = "test" if "测试" in question else "product" if "产品" in question else "code"
    metric = "bugs" if "bug" in question.lower() or "缺陷" in question else "lines" if "行数" in question or "代码量" in question else "repos" if "仓库" in question else "requirements" if "需求数量" in question else "events"
    return days, role, metric


def chart_data(data, metric):
    people = collections.defaultdict(lambda: {"events": 0, "lines": 0, "repos": set(), "requirements": set(), "bugs": set()})
    for event in data.get("events", []):
        row = people[event.get("actor") or "未识别成员"]
        row["events"] += 1
        if event.get("kind") == "bugReported":
            row["bugs"].add(event.get("externalId") or event["id"])
        row["lines"] += (event.get("additions") or 0) + (event.get("deletions") or 0)
        repository = event.get("repoId") or event.get("repo")
        if repository:
            row["repos"].add(str(repository))
        if event.get("kind") in ("requirementCreated", "requirementDelivered"):
            row["requirements"].add(event.get("externalId") or event["id"])
        row["requirements"].update(str(r.get("externalId")) for r in event.get("requirements", []) if r.get("externalId"))
    for bug in data.get("bugs", []) if data.get("role") != "test" else []:
        for person in bug.get("responsibles", []):
            people[person]["bugs"].add(bug["id"])
    rows = [(name, len(row[metric]) if isinstance(row[metric], set) else row[metric]) for name, row in people.items()]
    return sorted(rows, key=lambda row: (-row[1], row[0]))


def render_chart(data, metric, font_path):
    from PIL import Image, ImageDraw, ImageFont
    rows = chart_data(data, metric)
    shown = rows[:24]
    label = {"events": "活动记录", "lines": "已采集代码变动行数", "repos": "涉及仓库数", "requirements": "关联需求数", "bugs": "新增报告缺陷" if data.get("role") == "test" else "新增责任缺陷"}[metric]
    image = Image.new("RGB", (1280, max(520, 230+len(shown)*42)), "#F4F7F6")
    draw = ImageDraw.Draw(image)
    font = lambda size: ImageFont.truetype(font_path, size)
    draw.rounded_rectangle((24, 24, 1256, image.height-24), radius=24, fill="white")
    draw.text((60, 48), "Shepherd · 团队产出", fill="#274B44", font=font(30))
    draw.text((60, 94), f"{data.get('from', '')} — {data.get('through', '')}  ·  上海时间  ·  {label}", fill="#64746F", font=font(20))
    maximum = max((row[1] for row in shown), default=1) or 1
    for index, (name, count) in enumerate(shown):
        top = 148+index*42
        draw.text((60, top), name[:16], fill="#263F39", font=font(20))
        draw.rounded_rectangle((292, top+4, 1130, top+29), radius=6, fill="#EEF3F1")
        if count:
            draw.rounded_rectangle((292, top+4, 292+max(8, int(838*count/maximum)), top+29), radius=6, fill="#5B9A8B")
        draw.text((1145, top), str(count), fill="#315F52", font=font(18))
    if not shown:
        draw.text((60, 180), "所选范围暂无已采集记录", fill="#64746F", font=font(24))
    draw.text((60, image.height-66), f"已采集事实 · 展示前 {len(shown)} / {len(rows)} 位成员 · 完整数据见项目页面", fill="#73807C", font=font(16))
    buffer = io.BytesIO(); image.save(buffer, format="PNG", optimize=True)
    raw = buffer.getvalue()
    if len(raw) > 2_000_000:
        raise ValueError("统计图超过图片大小限制")
    return {"msgtype": "image", "image": {"base64": base64.b64encode(raw).decode(), "md5": hashlib.md5(raw, usedforsecurity=False).hexdigest()}}


def render_grounded_chart(spec, font_path):
    from PIL import Image, ImageDraw, ImageFont
    import math
    if spec.get("kind") == "bar":
        return render_grounded_bars(spec, font_path)
    if spec.get("kind") != "activity_heatmap":
        raise ValueError("unsupported_chart")
    if spec.get("expanded") and any(row.get("cells") for row in spec.get("rows", [])):
        return render_expanded_heatmap(spec, font_path)
    dates = spec.get("dates") or []
    rows = spec.get("rows") or []
    if not 1 <= len(dates) <= 366 or len(rows) > 500:
        raise ValueError("invalid_chart_size")
    shown = rows[:60]
    width = min(2400, max(1100, 260+len(dates)*52))
    height = max(320, 226+len(shown)*36)
    picture = Image.new("RGB", (width, height), "#F4F7F6")
    draw = ImageDraw.Draw(picture)
    font = lambda size: ImageFont.truetype(font_path, size)
    draw.rounded_rectangle((20, 20, width-20, height-20), radius=20, fill="white")
    draw.text((45, 40), str(spec.get("title") or "业务统计图")[:55], font=font(27), fill="#274B44")
    draw.text((45, 82), f"{spec.get('from', '')} — {spec.get('through', '')} · 上海时间 · {spec.get('metric', '')}", font=font(17), fill="#64746F")
    cell_width = (width-290)/len(dates)
    maximum = max((value for row in shown for value in row.get("values", []) if isinstance(value, (int, float)) and math.isfinite(value) and value >= 0), default=1) or 1
    interval = max(1, math.ceil(48/cell_width))
    for column, day in enumerate(dates):
        left = 240+column*cell_width
        if column % interval == 0:
            draw.text((left, 116), str(day)[5:10], font=font(14), fill="#64746F")
    for index, row in enumerate(shown):
        top = 145+index*36
        draw.text((45, top+3), str(row.get("label") or "未关联账号")[:12], font=font(19), fill="#263F39")
        for column, day in enumerate(dates):
            values = row.get("values", [])
            value = values[column] if column < len(values) else 0
            value = value if isinstance(value, (int, float)) and math.isfinite(value) and value > 0 else 0
            factor = 0.15+0.7*math.sqrt(value/maximum) if value else 0
            shade = tuple(round(245*(1-factor)+green*factor) for green in (72, 137, 119))
            left = 240+column*cell_width
            draw.rounded_rectangle((left, top, left+max(1, cell_width-3), top+29), radius=min(4, cell_width/3), fill=shade)
            if value and cell_width >= 28:
                draw.text((left+4, top+4), str(int(value)), font=font(15), fill="#183F34")
    footer = f"已采集记录 · 展示 {len(shown)} / {len(rows)} 行 · 空白表示未观测到记录"
    if spec.get("coverageComplete") is False:
        footer += " · 采集覆盖不完整"
    draw.text((45, height-65), footer, font=font(16), fill="#73807C")
    output = io.BytesIO(); picture.save(output, format="PNG", optimize=True)
    raw = output.getvalue()
    if len(raw) > 2_000_000:
        raise ValueError("chart_exceeds_image_limit")
    return {"msgtype": "image", "image": {"base64": base64.b64encode(raw).decode(), "md5": hashlib.md5(raw, usedforsecurity=False).hexdigest()}}


def render_expanded_heatmap(spec, font_path):
    from PIL import Image, ImageDraw, ImageFont
    from datetime import date
    import math
    dates, rows = spec.get("dates") or [], spec.get("rows") or []
    if not 1 <= len(dates) <= 366 or len(rows) > 500:
        raise ValueError("invalid_chart_size")
    shown = rows[:60]
    width = min(2800, max(1500, 380+len(dates)*220))
    row_height = 78
    height = max(350, 250+len(shown)*row_height)
    picture = Image.new("RGB", (width, height), "#F4F7F6")
    draw = ImageDraw.Draw(picture)
    font = lambda size: ImageFont.truetype(font_path, size)
    draw.rounded_rectangle((20, 20, width-20, height-20), radius=20, fill="white")
    draw.text((45, 38), str(spec.get("title") or "成员产出")[:55], font=font(29), fill="#274B44")
    draw.text((45, 84), f"{spec.get('from', '')} — {spec.get('through', '')} · 上海时间 · 展开指标", font=font(18), fill="#64746F")
    left_edge, right_edge = 220, width-195
    cell_width = (right_edge-left_edge)/len(dates)
    mode = spec.get("detailMode", "code")
    headers = ["提交", "需求", "Bug"] if mode == "code" else ["活动", "新建", "发布"] if mode == "product" else ["活动", "用例", "Bug"]
    for i, label in enumerate(headers):
        draw.text((right_edge+12+i*51, 126), label, font=font(16), fill="#73807C")
    for column, day in enumerate(dates):
        left = left_edge+column*cell_width
        weekday = date.fromisoformat(day).weekday()
        weekend = weekday >= 5
        if weekend: draw.rounded_rectangle((left, 114, left+cell_width-4, 163), radius=4, fill="#F4EEE7")
        if cell_width >= 40:
            draw.text((left+8, 116), day[5:], font=font(17), fill="#A46B36" if weekend else "#64746F")
            draw.text((left+8, 140), "周"+"一二三四五六日"[weekday], font=font(15), fill="#A46B36" if weekend else "#73807C")
    maximum = max((v for row in shown for v in row.get("values", []) if isinstance(v, (int, float)) and math.isfinite(v) and v > 0), default=1)
    def centered(text, left, top, size, color="#263F39"):
        face=font(size);box=draw.textbbox((0,0),text,font=face)
        draw.text((left+(cell_width-(box[2]-box[0]))/2,top),text,font=face,fill=color)
    def number(value): return "—" if value is None else f"{int(value):,}"
    for index, row in enumerate(shown):
        top=174+index*row_height
        draw.text((45,top+20),str(row.get("label") or "未关联账号")[:12],font=font(21),fill="#263F39")
        for column, day in enumerate(dates):
            left=left_edge+column*cell_width
            value=(row.get("values") or [])[column] if column<len(row.get("values") or []) else 0
            value=value if isinstance(value,(int,float)) and math.isfinite(value) and value>0 else 0
            factor=.12+.55*math.sqrt(value/maximum) if value else 0
            shade=tuple(round(245*(1-factor)+green*factor) for green in (72,137,119))
            draw.rounded_rectangle((left,top,left+max(1,cell_width-4),top+row_height-5),radius=4,fill=shade)
            cell=(row.get("cells") or [])[column] if column<len(row.get("cells") or []) else {}
            if cell_width < 70:
                if value: centered(str(int(value)),left,top+20,16)
                continue
            if not cell.get("count") and not cell.get("bugs"):
                centered("·",left,top+20,20,"#A0AAA6");continue
            heading=cell.get("lastTime") or "—"
            if cell.get("bugs"):
                badge="Bug "+str(cell["bugs"]);face=font(20);small=font(15)
                tw=draw.textbbox((0,0),heading,font=face)[2];bw=draw.textbbox((0,0),badge,font=small)[2]+8
                start=left+(cell_width-tw-bw-8)/2
                draw.text((start,top+5),heading,font=face,fill="#263F39")
                draw.rounded_rectangle((start+tw+7,top+7,start+tw+7+bw,top+29),radius=3,fill="#F4E8D9")
                draw.text((start+tw+11,top+8),badge,font=small,fill="#A46B36")
            else: centered(heading,left,top+5,20)
            if mode == "code":
                if cell.get("count"):
                    lines="行数未知" if cell.get("additions") is None and cell.get("deletions") is None else "+"+number(cell.get("additions"))+" / −"+number(cell.get("deletions"))
                    if cell.get("lineStatsIncomplete"): lines+="*"
                    centered(lines,left,top+31,15)
                if cell.get("requirements"): centered("关联需求 "+str(cell["requirements"]),left,top+52,15)
            else:
                centered(str(cell.get("count",0))+" 次活动",left,top+31,16)
                label=("新建 "+str(cell.get("created",0))+" · 发布 "+str(cell.get("published",0))) if mode=="product" else "用例 "+str(cell.get("testcases",0))
                centered(label,left,top+52,15)
        totals=row.get("totals") or {}
        values=[totals.get("count"),totals.get("requirements"),totals.get("bugs")] if mode=="code" else [totals.get("count"),totals.get("created"),totals.get("published")] if mode=="product" else [totals.get("count"),totals.get("testcases"),totals.get("bugs")]
        for i,value in enumerate(values): draw.text((right_edge+14+i*51,top+25),str(value) if value else "—",font=font(18),fill="#A46B36" if i==2 and mode!="product" and value else "#64746F")
    note="需求与 Bug 期间分别去重 · * 行数不全，仅为已知下限" if mode=="code" else "新建与发布分别计次，同一需求可能同时出现" if mode=="product" else "用例按活动计数，Bug 期间去重"
    footer=f"已采集记录 · 展示 {len(shown)} / {len(rows)} 行 · {note}"
    if spec.get("coverageComplete") is False: footer+=" · 采集覆盖待核验"
    draw.text((45,height-58),footer,font=font(17),fill="#73807C")
    output=io.BytesIO();picture.save(output,format="PNG",optimize=True);raw=output.getvalue()
    if len(raw)>2_000_000: raise ValueError("chart_exceeds_image_limit")
    return {"msgtype":"image","image":{"base64":base64.b64encode(raw).decode(),"md5":hashlib.md5(raw,usedforsecurity=False).hexdigest()}}


def render_grounded_bars(spec, font_path):
    from PIL import Image, ImageDraw, ImageFont
    import math
    labels, values = spec.get("labels") or [], spec.get("values") or []
    if len(labels) != len(values) or len(labels) > 500:
        raise ValueError("invalid_bar_chart")
    shown = list(zip(labels, values))[:40]
    image = Image.new("RGB", (1280, max(320, 210+len(shown)*36)), "#F4F7F6")
    draw = ImageDraw.Draw(image); font = lambda size: ImageFont.truetype(font_path, size)
    draw.rounded_rectangle((20, 20, 1260, image.height-20), radius=20, fill="white")
    draw.text((45, 40), str(spec.get("title") or "业务统计")[:45], font=font(27), fill="#274B44")
    draw.text((45, 82), f"{spec.get('from', '')} — {spec.get('through', '')} · {spec.get('metric', '')}", font=font(18), fill="#64746F")
    maximum = max((value for _, value in shown if isinstance(value, (int, float)) and math.isfinite(value) and value > 0), default=1)
    for index, (label, value) in enumerate(shown):
        value = value if isinstance(value, (int, float)) and math.isfinite(value) and value > 0 else 0
        top = 130+index*36
        draw.text((45, top), str(label)[:12], font=font(18), fill="#263F39")
        draw.rounded_rectangle((290, top+3, 1120, top+27), radius=5, fill="#EEF3F1")
        if value: draw.rounded_rectangle((290, top+3, 290+max(5, 830*value/maximum), top+27), radius=5, fill="#5B9A8B")
        draw.text((1140, top), str(int(value)), font=font(17), fill="#315F52")
    draw.text((45, image.height-60), f"已采集事实 · 展示 {len(shown)} / {len(labels)} 行 · 统计口径与网页一致", font=font(16), fill="#73807C")
    output = io.BytesIO(); image.save(output, format="PNG", optimize=True); raw=output.getvalue()
    if len(raw)>2_000_000:raise ValueError("chart_exceeds_image_limit")
    return {"msgtype":"image","image":{"base64":base64.b64encode(raw).decode(),"md5":hashlib.md5(raw,usedforsecurity=False).hexdigest()}}
