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


def clean_question(value):
    value = re.sub(r"<@[^>]+>", "", str(value or "")).strip()
    return re.sub(r"^@\S+\s+", "", value).strip()[:8000]


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
