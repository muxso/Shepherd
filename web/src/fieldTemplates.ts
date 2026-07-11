import type { FieldTemplateConfig, TemplateField, TemplateFieldType } from './api'

// 字段模板(MeterSphere 式):每个 kind 一份字段配置,决定创建表单的字段/顺序/必填/显隐。
// 系统字段在此注册(key 固定,label 走 i18n);自定义字段由用户在模板管理里添加。

export type TemplateKind = 'requirement' | 'functional-case' | 'bug'

export const TEMPLATE_KINDS: TemplateKind[] = ['requirement', 'functional-case', 'bug']

/** 每个 kind 恰好一份配置,模板行 name 固定用该值。 */
export const FIELDS_TEMPLATE_NAME = 'fields'

/** 系统字段注册项:默认顺序 = 数组序。 */
export interface SystemFieldDef {
  key: string
  /** i18n key(渲染时取);zh 为回落文案。 */
  labelKey: string
  labelZh: string
  type: TemplateFieldType
  /** 锁定 = 必填/显示开关禁用(如 title/name,永远必填可见)。 */
  locked?: boolean
  /** 默认必填(锁定字段隐含必填)。 */
  defaultRequired?: boolean
}

export const SYSTEM_FIELDS: Record<TemplateKind, SystemFieldDef[]> = {
  requirement: [
    { key: 'title', labelKey: 'req.title', labelZh: '标题', type: 'text', locked: true },
    { key: 'reqType', labelKey: 'req.reqType', labelZh: '类型', type: 'select' },
    { key: 'priority', labelKey: 'req.priority', labelZh: '优先级', type: 'select' },
    { key: 'tags', labelKey: 'req.tags', labelZh: '标签', type: 'multiselect' },
    { key: 'dueDate', labelKey: 'req.dueDate', labelZh: '截止日期', type: 'date' },
    { key: 'parentId', labelKey: 'req.parentReq', labelZh: '父需求', type: 'select' },
    { key: 'description', labelKey: 'req.description', labelZh: '需求描述', type: 'textarea' },
    // 验收标准列表:type 只作标记,渲染由需求页自己处理(逐条录入)。
    { key: 'criteria', labelKey: 'req.criteriaPlain', labelZh: '验收标准', type: 'textarea' },
  ],
  'functional-case': [
    { key: 'name', labelKey: 'func.caseName', labelZh: '用例名', type: 'text', locked: true },
    { key: 'module', labelKey: 'func.colModule', labelZh: '模块', type: 'text' },
    { key: 'priority', labelKey: 'func.colPriority', labelZh: '优先级', type: 'select' },
    { key: 'prerequisite', labelKey: 'func.prerequisite', labelZh: '前置条件', type: 'textarea' },
    // 步骤(步骤+预期):渲染由用例页自己处理(StepsEditor)。
    { key: 'steps', labelKey: 'func.stepsDesc', labelZh: '步骤描述', type: 'textarea' },
    { key: 'remark', labelKey: 'func.remark', labelZh: '备注', type: 'textarea' },
  ],
  bug: [
    { key: 'title', labelKey: 'bug.title', labelZh: '标题', type: 'text', locked: true },
  ],
}

export const systemFieldDef = (kind: TemplateKind, key: string): SystemFieldDef | undefined =>
  SYSTEM_FIELDS[kind].find((d) => d.key === key)

/** 系统字段是否锁定(必填/显示开关禁用)。 */
export const isLockedField = (kind: TemplateKind, f: TemplateField): boolean =>
  f.system && !!systemFieldDef(kind, f.key)?.locked

/** 字段显示名:系统字段走 i18n,自定义字段用 label(缺省回落 key)。 */
export const fieldLabel = (t: (k: string, d?: string) => string, kind: TemplateKind, f: TemplateField): string => {
  if (f.system) {
    const def = systemFieldDef(kind, f.key)
    if (def) return t(def.labelKey, def.labelZh)
  }
  return f.label || f.key
}

/** 内置默认配置:注册表全量系统字段,按注册顺序,全部显示。 */
export const defaultTemplateFields = (kind: TemplateKind): TemplateField[] =>
  SYSTEM_FIELDS[kind].map((d) => ({
    key: d.key,
    label: '',
    type: d.type,
    required: !!(d.locked || d.defaultRequired),
    enabled: true,
    system: true,
  }))

/**
 * 归一化已存配置:保留存储顺序;丢掉注册表里已不存在的系统字段;
 * 补齐注册表新增的系统字段(追加在末尾);锁定字段强制 required+enabled。
 */
export const normalizeFields = (kind: TemplateKind, config?: FieldTemplateConfig | null): TemplateField[] => {
  const saved = Array.isArray(config?.fields) ? config.fields : null
  if (!saved) return defaultTemplateFields(kind)
  const out: TemplateField[] = []
  const seen = new Set<string>()
  for (const f of saved) {
    if (!f || typeof f.key !== 'string' || seen.has(f.key)) continue
    if (f.system) {
      const def = systemFieldDef(kind, f.key)
      if (!def) continue
      out.push({
        key: def.key,
        label: '',
        type: def.type,
        required: def.locked ? true : !!f.required,
        enabled: def.locked ? true : f.enabled !== false,
        system: true,
      })
    } else {
      out.push({
        key: f.key,
        label: f.label || f.key,
        type: f.type,
        required: !!f.required,
        enabled: f.enabled !== false,
        system: false,
        options: f.options?.length ? [...f.options] : undefined,
      })
    }
    seen.add(f.key)
  }
  for (const d of SYSTEM_FIELDS[kind]) {
    if (!seen.has(d.key)) {
      out.push({ key: d.key, label: '', type: d.type, required: !!(d.locked || d.defaultRequired), enabled: true, system: true })
    }
  }
  return out
}

/** 自定义字段 key:c_ 前缀随机短 id。 */
export const newCustomFieldKey = (): string => `c_${Math.random().toString(36).slice(2, 8)}`
