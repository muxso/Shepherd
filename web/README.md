# Shepherd Web · 接口测试管理

React + TypeScript + Vite + Ant Design 实现的前端,参考 MeterSphere 的「左树/列表 + 右 Tab 详情」布局,对接 Shepherd Rust server 的接口测试面。

## 功能

- **登录**:`POST /auth/login`(默认 `admin`)。
- **项目切换 / 新建项目**:顶栏选择项目;无项目时可一键新建组织 + 项目。
- **接口定义**:左侧列表(方法/协议彩签、搜索),右侧 Tab:
  - 基本信息
  - 接口用例:新建用例、选资源池后运行(并行/串行)、查看执行历史
  - Mock:新建/列出 Mock 期望
  - 导入:粘贴 OpenAPI 3.x / Swagger 2.0 JSON 导入
- **场景用例**:新建场景、添加步骤(CASE / REQUEST 内联 / 子场景)、运行场景。

## 开发

前置:后端 server 跑在 `127.0.0.1:9180`(见仓库根 `scripts/`)。Vite dev 已配置 proxy,前端零 CORS。

```bash
cd web
npm install
npm run dev          # http://localhost:5173,API 自动 proxy 到 :9180
```

后端端口不同?用环境变量覆盖:

```bash
SHEPHERD_API=http://127.0.0.1:9185 npm run dev
```

## 构建

```bash
npm run build        # tsc -b && vite build → dist/
npm run preview      # 本地预览产物
```

## 结构

```
src/
  api.ts                 REST 客户端 + 类型(镜像后端 DTO,camelCase)
  context.tsx            登录态 + 组织/项目状态
  components/
    AppShell.tsx         顶栏 + 左侧模块导航(MeterSphere 风)
    NewProjectModal.tsx  新建项目(含内联新建组织)
    tags.ts              方法/状态/结果 → 颜色
  pages/
    Login.tsx
    ApiDefinitions.tsx   接口定义:左列表 + 右 Tab
    CasesPanel.tsx       接口用例 + 运行(选资源池)+ 执行历史
    MocksPanel.tsx       Mock 管理
    Scenarios.tsx        场景:列表 + 步骤编排 + 运行
```
