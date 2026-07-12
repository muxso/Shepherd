import type { FieldTemplateConfig, TemplateField, TemplateFieldType } from './api'

// Field templates (MeterSphere-style): one field config per kind, driving each create form's
// fields/order/required/visibility. System fields are registered here (fixed key, label via i18n);
// custom fields are added by users in template management.

export type TemplateKind = 'requirement' | 'functional-case' | 'bug'

export const TEMPLATE_KINDS: TemplateKind[] = ['requirement', 'functional-case', 'bug']

/** Exactly one config per kind; template rows always use this name. */
export const FIELDS_TEMPLATE_NAME = 'fields'

/** System field registration entry; default order = array order. */
export interface SystemFieldDef {
  key: string
  /** i18n key (resolved at render); zh is the fallback label. */
  labelKey: string
  labelZh: string
  type: TemplateFieldType
  /** Locked = required/visible toggles disabled (e.g. title/name: always required and visible). */
  locked?: boolean
  /** Required by default (locked fields are implicitly required). */
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
    // Acceptance criteria list: type is only a marker; the requirement page renders it itself (one entry per criterion).
    { key: 'criteria', labelKey: 'req.criteriaPlain', labelZh: '验收标准', type: 'textarea' },
  ],
  'functional-case': [
    { key: 'name', labelKey: 'func.caseName', labelZh: '用例名', type: 'text', locked: true },
    { key: 'module', labelKey: 'func.colModule', labelZh: '模块', type: 'text' },
    { key: 'priority', labelKey: 'func.colPriority', labelZh: '优先级', type: 'select' },
    { key: 'prerequisite', labelKey: 'func.prerequisite', labelZh: '前置条件', type: 'textarea' },
    // Steps (step + expected): rendered by the case page itself (StepsEditor).
    { key: 'steps', labelKey: 'func.stepsDesc', labelZh: '步骤描述', type: 'textarea' },
    { key: 'remark', labelKey: 'func.remark', labelZh: '备注', type: 'textarea' },
  ],
  bug: [
    { key: 'title', labelKey: 'bug.title', labelZh: '标题', type: 'text', locked: true },
  ],
}

export const systemFieldDef = (kind: TemplateKind, key: string): SystemFieldDef | undefined =>
  SYSTEM_FIELDS[kind].find((d) => d.key === key)

/** Whether a system field is locked (required/visible toggles disabled). */
export const isLockedField = (kind: TemplateKind, f: TemplateField): boolean =>
  f.system && !!systemFieldDef(kind, f.key)?.locked

/** Field display name: system fields via i18n; custom fields use label, falling back to key. */
export const fieldLabel = (t: (k: string, d?: string) => string, kind: TemplateKind, f: TemplateField): string => {
  if (f.system) {
    const def = systemFieldDef(kind, f.key)
    if (def) return t(def.labelKey, def.labelZh)
  }
  return f.label || f.key
}

/** Built-in default config: all registered system fields, in registration order, all visible. */
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
 * Normalize a stored config: keep the stored order; drop system fields no longer in the registry;
 * append newly registered system fields at the end; force locked fields to required+enabled.
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

/** Custom field key: c_ prefix + random short id. */
export const newCustomFieldKey = (): string => `c_${Math.random().toString(36).slice(2, 8)}`
