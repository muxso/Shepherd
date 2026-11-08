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
