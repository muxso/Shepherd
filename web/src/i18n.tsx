import { createContext, useContext, useMemo, useState, type ReactNode } from 'react'

export type Lang = 'zh' | 'en'
const KEY = 'shepherd.lang'

// 扁平字典:key → {zh,en}。逐页迁移文案到此处即可双语;未登记的 key 回落原文。
const DICT: Record<string, { zh: string; en: string }> = {
  // 顶栏 / 一级导航
  'top.api': { zh: '接口测试', en: 'API Testing' },
  'top.exec': { zh: '计划与执行', en: 'Plan & Run' },
  'top.orch': { zh: '需求与编排', en: 'Requirement & Orchestration' },
  'top.sys': { zh: '系统管理', en: 'System' },
  'top.project': { zh: '项目', en: 'Project' },
  'top.newProject': { zh: '新建项目', en: 'New Project' },
  'top.logout': { zh: '退出登录', en: 'Sign out' },
  // 侧栏分组
  'g.asset': { zh: '测试资产', en: 'Test Assets' },
  'g.exec': { zh: '计划与执行', en: 'Plan & Run' },
  'g.orch': { zh: '需求与编排', en: 'Requirement & Orchestration' },
  'g.sys': { zh: '系统管理', en: 'System' },
  // 侧栏菜单项
  'm.definition': { zh: '接口定义', en: 'API Definitions' },
  'm.scenario': { zh: '场景用例', en: 'Scenarios' },
  'm.functional': { zh: '功能用例', en: 'Functional Cases' },
  'm.plan': { zh: '测试计划', en: 'Test Plans' },
  'm.perf': { zh: '性能压测', en: 'Performance' },
  'm.environment': { zh: '环境', en: 'Environments' },
  'm.pool': { zh: '资源池', en: 'Resource Pools' },
  'm.requirement': { zh: '需求与编排', en: 'Requirements' },
  'm.bug': { zh: '缺陷', en: 'Bugs' },
  'm.skill': { zh: '技能', en: 'Skills' },
  'm.org': { zh: '组织', en: 'Organizations' },
  'm.role': { zh: '角色', en: 'Roles' },
  'm.user': { zh: '用户', en: 'Users' },
  'm.proj': { zh: '项目', en: 'Projects' },
  'm.mcp': { zh: 'MCP 工具', en: 'MCP Tools' },
  // 通用动作
  'a.new': { zh: '新建', en: 'New' },
  'a.create': { zh: '创建', en: 'Create' },
  'a.save': { zh: '保存', en: 'Save' },
  'a.cancel': { zh: '取消', en: 'Cancel' },
  'a.delete': { zh: '删除', en: 'Delete' },
  'a.refresh': { zh: '刷新', en: 'Refresh' },
  'a.run': { zh: '运行', en: 'Run' },
  'a.send': { zh: '发送', en: 'Send' },
  'a.search': { zh: '搜索', en: 'Search' },
  'a.import': { zh: '导入', en: 'Import' },
  'a.detail': { zh: '详情', en: 'Detail' },
  'common.selectProject': { zh: '请先在顶部选择项目', en: 'Select a project first' },
  'common.empty': { zh: '暂无数据', en: 'No data' },
}

interface Ctx {
  lang: Lang
  setLang: (l: Lang) => void
  t: (key: string, fallback?: string) => string
}
const LangCtx = createContext<Ctx | null>(null)

export function LangProvider({ children }: { children: (lang: Lang) => ReactNode }) {
  const [lang, setLangState] = useState<Lang>((localStorage.getItem(KEY) as Lang) || 'zh')
  const setLang = (l: Lang) => {
    setLangState(l)
    localStorage.setItem(KEY, l)
  }
  const value = useMemo<Ctx>(
    () => ({ lang, setLang, t: (key, fallback) => DICT[key]?.[lang] ?? fallback ?? key }),
    [lang],
  )
  return <LangCtx.Provider value={value}>{children(lang)}</LangCtx.Provider>
}

export function useI18n() {
  const v = useContext(LangCtx)
  if (!v) throw new Error('useI18n must be used within LangProvider')
  return v
}
