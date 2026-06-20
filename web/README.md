# Shepherd Web · 全平台前端

React + TypeScript + Vite + Ant Design 实现,参考 MeterSphere「顶栏 + 分组侧栏 + 列表/详情」布局,覆盖 Shepherd Rust server 的全部业务域。

## 启动(必须前后端同时跑)

后端(终端 1):
```bash
cd /Users/zhiyi/Code/rust/Shepherd
DATABASE_URL=postgres://msuser:mspass@127.0.0.1:55432/mstest \
MS_BIND=127.0.0.1:9180 MS_ADMIN_PASSWORD=s3cret RUST_LOG=error \
./target/debug/server
```
前端(终端 2):
```bash
cd web && npm install && npm run dev      # http://localhost:5173,API proxy 到 :9180
```
> 改了 `vite.config.ts` 后必须重启 dev,代理才生效。后端端口不同用 `SHEPHERD_API=... npm run dev`。

登录默认 `admin` / `s3cret`。首次无项目时,右上角「+」新建项目(可顺手建组织)。

## 模块覆盖

| 分组 | 模块 | 能力 |
|------|------|------|
| 测试资产 | 接口定义 | 三栏(模块树/列表/详情)、新建、导入 OpenAPI、用例(选资源池运行+历史)、Mock |
| | 场景用例 | 列表、步骤编排(CASE/REQUEST/子场景)、运行 |
| | 功能用例 | 列表 + 新建 |
| 计划与执行 | 测试计划 | 新建、挂用例、执行、统计卡、Markdown 报告、定时(cron) |
| | 性能压测 | 发起(并发/迭代)、吞吐与延迟分位报告 |
| | 环境 / 资源池 | 列表 + 新建 |
| 需求与编排 | 需求 | 新建、版本、定基线、自动拆分 |
| | 拆分/交付/验证 | 任务图、并行运行、按任务派发、交付事件、验证覆盖报告 |
| | 缺陷 | 新建 + 状态流转 |
| | 技能 | 新建 + 组合 |
| 系统管理 | 组织 / 角色 / 用户 / 项目 | 列表 + 新建 |
| | MCP 工具 | 只读列出 server 暴露的 JSON-RPC 工具 |

## 已知约束

- **测试计划 / 压测报告 / 需求 / 缺陷 / 技能 / 拆分图** 后端目前**没有「按项目列表」端点**,前端用 `localStorage` 维护一份按项目隔离的本地注册表(`src/registry.ts`)以提供可浏览列表。后端补 list 端点后可平滑替换为真列表。
- 报告导出统一 **Markdown**(不提供 PDF)。
- 接口用例「发送/响应调试台」未做:现有 `/runner/probe` 会派发到 runner-agent(本地未起),需后端新增进程内 `POST /api/debug/send` 才能直连。

## 结构

```
src/
  api.ts            REST 客户端 + 类型(镜像后端 DTO,camelCase);projectId 为空的作用域调用自动空返回
  context.tsx       登录态 + 组织/项目
  registry.ts       无 list 端点资源的本地注册表
  components/
    AppShell.tsx    顶栏(一级导航+面包屑)+ 分组侧栏
    CrudResource.tsx 配置化 CRUD 引擎(组织/角色/用户/功能用例/环境/资源池复用)
    NewProjectModal.tsx / tags.ts
  pages/
    Login / ApiDefinitions / Scenarios / TestPlans / Perf
    Requirements / Orchestration / Bugs / Skills / Mcp / resources
```
