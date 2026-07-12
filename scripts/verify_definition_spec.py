#!/usr/bin/env python3
"""集成验证: 接口定义「定义」编辑器 spec 全字段往返 (基本信息/请求体content-type/子标签)。

登录 → 找/建项目 → 新建接口 → PUT 一份覆盖所有子标签与请求体 content-type 的 spec
→ GET 读回 → 逐字段比对。证明前端「定义」编辑器写入的 spec 经后端无损往返。
"""
import json
import sys
import urllib.request

BASE = "http://127.0.0.1:9180"


def req(method, path, token=None, body=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    data = json.dumps(body).encode() if body is not None else None
    r = urllib.request.Request(BASE + path, data=data, headers=headers, method=method)
    with urllib.request.urlopen(r) as resp:
        raw = resp.read().decode()
        return resp.status, (json.loads(raw) if raw.strip() else None)


def req_json(method, path, token=None, body=None):
    """req() but asserts a JSON object/array came back (for the type checker)."""
    status, payload = req(method, path, token, body)
    assert payload is not None, f"{method} {path} returned empty body"
    return status, payload


def main():
    _, login = req_json("POST", "/auth/login", body={"username": "admin", "password": "s3cret"})
    token = login["token"]
    print(f"✓ 登录成功 token={token[:10]}…")

    _, projects = req_json("GET", "/api/project", token)
    if not projects:
        print("✗ 无项目,先在 UI 建项目"); sys.exit(1)
    pid = projects[0]["id"]
    print(f"✓ 使用项目 {projects[0].get('name')} ({pid[:8]}…)")

    # Create the API definition
    _, d = req_json("POST", "/api/definition", token, {
        "projectId": pid, "name": "[验证] 定义编辑器全字段", "protocol": "HTTP",
        "method": "POST", "path": "/api/verify/{id}",
    })
    did = d["id"]
    print(f"✓ 新建接口 num={d.get('num')} id={did[:8]}…")

    # Spec covering every sub-tab plus the request-body content-type
    spec = {
        # Basic info
        "description": "验证用接口·中文描述",
        "tags": ["smoke", "回归"],
        # Request-headers sub-tab
        "requestHeaders": [{"name": "X-Trace", "value": "abc", "desc": "链路"}],
        # Query sub-tab
        "requestQuery": [{"name": "page", "value": "1", "desc": "页码"}],
        # REST-params sub-tab
        "restParams": [{"name": "id", "value": "42", "desc": "资源 id"}],
        # Request body content-type: json + schema tree
        "bodyType": "json",
        "requestBody": '{"user":"admin","age":30,"vip":true}',
        "bodySchema": [
            {"name": "user", "type": "string", "value": "admin", "description": "必填 · 用户名"},
            {"name": "age", "type": "integer", "value": "30", "description": "选填"},
            {"name": "vip", "type": "boolean", "value": "true"},
        ],
        "formBody": [{"name": "f1", "value": "v1", "desc": ""}],
        # Auth sub-tab
        "auth": {"type": "bearer", "token": "TKN"},
        # Response content
        "responses": [{"status": 200, "body": '{"ok":true}', "headers": [{"name": "Content-Type", "value": "application/json"}]}],
        # Processors / assertions (debug-mode sub-tabs)
        "preProcessors": [{"type": "Wait", "args": {"ms": 100}}],
        "postProcessors": [{"type": "Extract", "args": {"extractors": [{"variable": "tok", "kind": "JSONPath", "expression": "$.token"}]}}],
        "assertions": [{"type": "StatusCode", "condition": "eq", "expected": "200"}],
    }
    st, _ = req("PUT", f"/api/definition/{did}/spec", token, {"spec": spec})
    print(f"✓ 保存 spec → HTTP {st}")

    # Read back
    _, got = req_json("GET", f"/api/definition/{did}", token)
    back = got.get("spec") or {}

    print("\n=== 子标签 / content-type 往返比对 ===")
    ok = True
    for key in spec:
        same = back.get(key) == spec[key]
        ok = ok and same
        mark = "✓" if same else "✗"
        print(f"  {mark} {key}")
        if not same:
            print(f"      期望: {json.dumps(spec[key], ensure_ascii=False)}")
            print(f"      读回: {json.dumps(back.get(key), ensure_ascii=False)}")

    # Cleanup
    req("DELETE", f"/api/definition/{did}", token)
    print(f"\n{'✅ 全部字段无损往返' if ok else '❌ 有字段丢失/变形'} (已删除验证数据)")
    sys.exit(0 if ok else 2)


if __name__ == "__main__":
    main()
