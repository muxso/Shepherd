# 测试计划报告 · Shepherd 自举回归(全量)

状态:COMPLETED · 结论:**通过**

## 报告分析

- 用例总数:13
- 报告总耗时:71 ms
- 执行率:100.0%
- 通过率:100.0%
- 断言通过率:100.0% (16/16)

## 用例状态分布

- 通过 13 · 失败 0 · 误报 0 · 阻塞 0 · 未执行 0

## 报告明细

### 1. 用例1 `[通过]` — 状态码 200 · 响应时间 2 ms · 响应大小 2 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 200 | 等于 | 200 | 成功 |  |
| 响应体 | ok | 包含 | ok | 成功 |  |

**响应体**

```
ok
```

### 2. 用例10 `[通过]` — 状态码 201 · 响应时间 5 ms · 响应大小 148 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 201 | 等于 | 201 | 成功 |  |

**响应体**

```
{"id":"853d3201-7836-4b30-a425-bc2d74b2c112","projectId":"selfreg-7734","name":"S","description":"","instructions":"x","includes":[],"enabled":true}
```

### 3. 用例11 `[通过]` — 状态码 201 · 响应时间 6 ms · 响应大小 99 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 201 | 等于 | 201 | 成功 |  |
| 响应体 | {"id":"5a8ca567-af3c-4fc3-beee-bdcff67528fa","projectId":"se… | 包含 | NEW | 成功 |  |

**响应体**

```
{"id":"5a8ca567-af3c-4fc3-beee-bdcff67528fa","projectId":"selfreg-7734","title":"b","status":"NEW"}
```

### 4. 用例12 `[通过]` — 状态码 201 · 响应时间 2 ms · 响应大小 112 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 201 | 等于 | 201 | 成功 |  |

**响应体**

```
{"id":"afa2055c-5482-4b74-964c-7b9c574ecf03","projectId":"selfreg-7734","name":"Sc","status":"DRAFT","steps":[]}
```

### 5. 用例2 `[通过]` — 状态码 200 · 响应时间 1 ms · 响应大小 0 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 200 | 等于 | 200 | 成功 |  |

### 6. 用例3 `[通过]` — 状态码 200 · 响应时间 25 ms · 响应大小 74583 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 200 | 等于 | 200 | 成功 |  |
| 响应体 | {"openapi":"3.1.0","info":{"title":"Shepherd API","descripti… | 包含 | openapi | 成功 |  |

**响应体**

```
{"openapi":"3.1.0","info":{"title":"Shepherd API","description":"AI 研发监督平台 REST API","license":{"name":"GPL-2.0","identifier":"GPL-2.0"},"version":"0.0.1"},"paths":{"/api/batch-run":{"post":{"tags":["api-test"],"operationId":"batch_run","requestBody":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/BatchRunRequest"}}},"required":true},"responses":{"200":{"description":"","content":{"application/json":{"schema":{"$ref":"#/components/schemas/BatchRunResponse"}}}},"400":{"description":""},"409":{"description":""}}}},"/api/case":{"get":{"tags":["api-definition"],"operationId":"list_project_cases","parameters":[{"name":"projectId","in":"path","required":true,"schema":{"type":"string"}},{"name":"current","in":"path","required":true,"schema":{"type":"integer","format":"int32","minimum":0}},{"name":"pageSize","in":"path","required":true,"schema":{"type":"integer","format":"int32","minimum":0}}],"responses":{"200":{"description":"","content":{"application/json":{"schema":{"$ref":"#/components/schemas/ApiCasePageResponse"}}}},"400":{"description":""}}},"post":{"tags":["api-definition"],"operationId":"create_standalone_case","requestBody":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/StandaloneCaseBody"}}},"required":true},"responses":{"201":{"description":"","content":{"application/json":{"schema":{"$ref":"#/components/schemas/ApiCaseResponse"}}}},"400":{"description":""},"401":{"description":""},"403":{"description":""}},"security":[{"bearer":[]}]}},"/api/case/{caseId}/executions":{"get":{"tags":["api-test"],"operationId":"list_executions","parameters":[{"name":"caseId","in":"path","description":"用例 id","required":true,"schema":{"type":"string"}},{"name":"current","in":"path","required":true,"schema":{"type":"integer","format":"int32","minimum":0}},{"name":"pageSize","in":"path","required":true,"schema":{"type":"integer","format":"int32","minimum":0}}],"responses":{"200":{"description":"","content":{"application/json":{"schema":…
```

### 7. 用例4 `[通过]` — 状态码 200 · 响应时间 6 ms · 响应大小 63 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 200 | 等于 | 200 | 成功 |  |

**响应体**

```
{"total":0,"current":1,"pageSize":10,"totalPages":0,"items":[]}
```

### 8. 用例5 `[通过]` — 状态码 200 · 响应时间 6 ms · 响应大小 596 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 200 | 等于 | 200 | 成功 |  |

**响应体**

```
{"total":6,"current":1,"pageSize":5,"totalPages":2,"items":[{"id":"1b0c6288-ee23-41e7-87f2-ea61be6fc765","name":"cov-23004-u","email":"cov-23004@x.com","enable":true},{"id":"58f8c460-5f07-41d3-84a5-f8818d21fd31","name":"cov-17414-u","email":"cov-17414@x.com","enable":true},{"id":"9062cf97-a79e-4e3c-9ede-c290cd92aa03","name":"cov-17514-u","email":"cov-17514@x.com","enable":true},{"id":"bdc1ce70-7a81-4d32-a7d9-dc2e8d454cd2","name":"cov-23311-u","email":"cov-23311@x.com","enable":true},{"id":"ee24d525-75f0-4dd9-8631-a2c0b6faab6a","name":"cov-17322-u","email":"cov-17322@x.com","enable":true}]}
```

### 9. 用例6 `[通过]` — 状态码 200 · 响应时间 2 ms · 响应大小 302 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 200 | 等于 | 200 | 成功 |  |

**响应体**

```
[{"id":"18fcf6ca-bcb4-4176-b540-d19ac1a7e0d0","projectId":"selfreg-7734","name":"自举环境","baseUrl":"http://127.0.0.1:9180","headers":[{"name":"Authorization","value":"Bearer f4aca275-d934-496b-8af4-2c1d5290ba46"},{"name":"Content-Type","value":"application/json"}],"variables":{},"enabled":true}]
```

### 10. 用例7 `[通过]` — 状态码 201 · 响应时间 6 ms · 响应大小 102 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 201 | 等于 | 201 | 成功 |  |

**响应体**

```
{"id":"9537d6ce-a729-4bfc-8651-24d0597ae5ea","organizationId":"selfreg-7734","name":"P","enable":true}
```

### 11. 用例8 `[通过]` — 状态码 201 · 响应时间 8 ms · 响应大小 226 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 201 | 等于 | 201 | 成功 |  |
| 响应体 | {"id":"2498cc84-7c70-431b-aad0-51db96720d27","projectId":"se… | 包含 | id | 成功 |  |

**响应体**

```
{"id":"2498cc84-7c70-431b-aad0-51db96720d27","projectId":"selfreg-7734","title":"登录","status":"DRAFT","baselineVersion":1,"latestVersion":1,"versions":[{"version":1,"description":"","acceptanceCriteria":["登录成功"]}]}
```

### 12. 用例9 `[通过]` — 状态码 201 · 响应时间 2 ms · 响应大小 119 bytes

| 断言项 | 返回值 | 匹配条件 | 匹配值 | 状态 | 原因 |
|---|---|---|---|---|---|
| 状态码 | 201 | 等于 | 201 | 成功 |  |

**响应体**

```
{"id":"b75c45d0-625b-45b5-bcea-a67f05c26c63","projectId":"selfreg-7734","name":"P","type":"TEST_PLAN","groupId":"NONE"}
```

### 13. 需求冒烟编排 `[通过]` — 响应时间 0 ms · 响应大小 0 bytes

**步骤**

- [通过] GET /healthz（接口用例 · 状态码 200 · 0 ms）
- [通过] GET /healthz（请求 · 状态码 200 · 0 ms）
