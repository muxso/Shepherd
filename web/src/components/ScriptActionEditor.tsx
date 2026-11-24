import { useEffect, useState } from 'react'
import { Input, Select, Space, Typography } from 'antd'
import { request } from '../api'

export type ScriptAction = {
  mode: 'shared' | 'custom'
  scriptId?: string
  revision?: number
  language?: 'python' | 'javascript'
  code?: string
  inputText: string
  output: string
}

export const newScriptAction = (): ScriptAction => ({
  mode: 'shared', inputText: '{}', output: 'scriptResult',
})

export function scriptActionPayload(action: ScriptAction) {
  let input: unknown
  try { input = JSON.parse(action.inputText) } catch { return null }
  if (!input || typeof input !== 'object' || Array.isArray(input) || !/^(?!__)[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(action.output)) return null
  const source = action.mode === 'shared'
    ? action.scriptId && action.revision && { scriptId: action.scriptId, revision: action.revision }
    : action.code?.trim() && { language: action.language ?? 'javascript', code: action.code }
  return source ? { mode: action.mode, ...source, input, output: action.output } : null
}

type Script = { id: string; name: string; language: string; revision: number }
type Revision = { revision: number }

export default function ScriptActionEditor({ projectId, value, onChange }: {
  projectId: string
  value?: ScriptAction
  onChange?: (value: ScriptAction) => void
}) {
  const action = value ?? newScriptAction()
  const [scripts, setScripts] = useState<Script[]>([])
  const [versions, setVersions] = useState<Revision[]>([])
  const [error, setError] = useState('')
  const path = `/project/${encodeURIComponent(projectId)}/scripts`
  useEffect(() => {
    if (!projectId || action.mode !== 'shared') return
    let live = true
    request<Script[]>('GET', path).then(rows => { if (live) { setScripts(rows); setError('') } })
      .catch(() => { if (live) setError('无法读取项目公共脚本') })
    return () => { live = false }
  }, [path, projectId, action.mode])
  useEffect(() => {
    setVersions([])
    if (!action.scriptId || action.mode !== 'shared') return
    let live = true
    request<Revision[]>('GET', `${path}/${encodeURIComponent(action.scriptId)}`)
      .then(rows => { if (live) setVersions(rows) }).catch(() => {})
    return () => { live = false }
  }, [path, action.scriptId, action.mode])
  const patch = (fields: Partial<ScriptAction>) => onChange?.({ ...action, ...fields })
  const versionNumbers = [...new Set([...versions.map(item => item.revision), ...(action.revision ? [action.revision] : [])])]
  return <Space direction="vertical" style={{ width: '100%' }}>
    <Select aria-label="脚本来源" value={action.mode} onChange={mode => patch({ mode })}
      options={[{ value: 'shared', label: '引用公共脚本' }, { value: 'custom', label: '自定义脚本' }]} />
    {action.mode === 'shared' ? <>
      <Select aria-label="公共脚本" style={{ width: '100%' }} value={action.scriptId} placeholder="选择项目公共脚本"
        options={scripts.map(script => ({ value: script.id, label: `${script.name} · ${script.language}` }))}
        onChange={scriptId => patch({ scriptId, revision: scripts.find(script => script.id === scriptId)?.revision })} />
      <Select aria-label="脚本版本" style={{ width: 130 }} value={action.revision} placeholder="选择版本"
        options={versionNumbers.map(value => ({ value, label: `v${value}` }))} onChange={revision => patch({ revision })} />
      {error && <Typography.Text type="danger">{error}</Typography.Text>}
    </> : <>
      <Select aria-label="脚本语言" value={action.language ?? 'javascript'} onChange={language => patch({ language })}
        options={[{ value: 'javascript', label: 'JavaScript' }, { value: 'python', label: 'Python' }]} />
      <Input.TextArea aria-label="脚本源码" className="ms-mono" rows={7} maxLength={65536}
        value={action.code ?? ''} onChange={event => patch({ code: event.target.value })} />
    </>}
    <Typography.Text>输入参数（JSON）</Typography.Text>
    <Input.TextArea aria-label="脚本输入参数" className="ms-mono" rows={3} maxLength={65536}
      value={action.inputText} onChange={event => patch({ inputText: event.target.value })} />
    <Typography.Text>输出变量</Typography.Text>
    <Input aria-label="脚本输出变量" value={action.output} onChange={event => patch({ output: event.target.value })} />
    <Typography.Text type="secondary">标准输出须为一份 JSON。后续请求或断言可用 { `{{${action.output || 'scriptResult'}#/字段名}}` } 引用。</Typography.Text>
  </Space>
}
