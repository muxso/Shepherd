import { useEffect, useRef, useState } from 'react'
import { Alert, Button, Checkbox, Collapse, Form, Input, Select, Space, Table, Tabs, Tag } from 'antd'
import { PlusOutlined } from '@ant-design/icons'
import { ApiError, request } from '../api'
import { AccessDenied, PageBody, PageContainer, PageHeader, ReadError, SelectProjectEmpty } from '../components/Page'
import EditDrawer from '../components/EditDrawer'
import { useApp } from '../context'
import { message } from '../feedback'
import { copyToClipboard } from '../clipboard'

type Language = 'python' | 'javascript'
type Script = { id: string; name: string; language: Language; revision: number; updatedAt: string; description?: string; tags?: string[]; testStatus?: string | null }
type Revision = { revision: number; name: string; createdBy: string; createdAt: string }
type ParameterField = { name: string; defaultJson?: string; description?: string; required?: boolean }
type ScriptDraft = { name: string; language: Language; source: string; description?: string; tags?: string[]; parameters?: ParameterField[] }
type RevisionSource = Pick<ScriptDraft, 'source' | 'description' | 'tags'> & {
  parameters?: (Omit<ParameterField, 'defaultJson'> & { defaultValue?: unknown })[]
  testStatus?: string | null
}
type DebugResult = { stdout: string; stderr: string; exitCode: number | null; timedOut: boolean; durationMs: number; outputLimited: boolean }

function parseParameters(fields: ParameterField[] = []) {
  return fields.map(({ defaultJson, ...field }) => ({
    name: field.name.trim(), description: field.description ?? '', required: field.required ?? false,
    ...(defaultJson?.trim() ? { defaultValue: JSON.parse(defaultJson) } : {}),
  }))
}

const labels: Record<Language, string> = { python: 'Python', javascript: 'JavaScript' }

export default function ProjectScripts() {
  const { projectId } = useApp()
  return projectId ? <ScriptLibrary key={projectId} projectId={projectId} /> : <SelectProjectEmpty />
}

function ScriptLibrary({ projectId }: { projectId: string }) {
  const path = `/project/${encodeURIComponent(projectId)}/scripts`
  const [form] = Form.useForm<ScriptDraft>()
  const [items, setItems] = useState<Script[]>([])
  const [loading, setLoading] = useState(true)
  const [listError, setListError] = useState<unknown>()
  const [saving, setSaving] = useState(false)
  const [opened, setOpened] = useState<Script | 'new' | null>(null)
  const [selected, setSelected] = useState<number | undefined>()
  const [versions, setVersions] = useState<Revision[]>([])
  const [sourceReady, setSourceReady] = useState(false)
  const [search, setSearch] = useState('')
  const [tab, setTab] = useState('source')
  const [input, setInput] = useState('{}')
  const [running, setRunning] = useState(false)
  const [result, setResult] = useState<{ data: DebugResult; target: string } | null>(null)
  const [debugError, setDebugError] = useState('')
  const [versionStatus, setVersionStatus] = useState<string | null>(null)
  const [dirty, setDirty] = useState(false)
  const generation = useRef(0)
  const clearRun = () => {
    generation.current++
    setResult(null)
    setDebugError('')
    setRunning(false)
  }
  const script = opened && opened !== 'new' ? opened : null
  const readOnly = !!script && selected !== script.revision

  useEffect(() => {
    let live = true
    request<Script[]>('GET', path).then(rows => { if (live) setItems(rows) })
      .catch(error => { if (live) setListError(error) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [path])

  useEffect(() => {
    if (!script) return
    let live = true
    request<Revision[]>('GET', `${path}/${script.id}`).then(rows => { if (live) setVersions(rows) })
      .catch(() => { if (live) message.error('读取版本历史失败') })
    return () => { live = false }
  }, [path, script])

  useEffect(() => {
    if (!script || selected == null) return
    let live = true
    setSourceReady(false)
    request<RevisionSource>('GET', `${path}/${script.id}/revisions/${selected}`)
      .then(({ source, description, tags, parameters, testStatus }) => {
        if (!live) return
        form.setFieldsValue({ source, description: description ?? '', tags: tags ?? [], parameters: parameters?.map(({ defaultValue, ...parameter }) => ({
          ...parameter, defaultJson: defaultValue == null ? '' : JSON.stringify(defaultValue),
        })) ?? [] })
        setDirty(false)
        setVersionStatus(testStatus ?? null)
        setSourceReady(true)
      })
      .catch(() => { if (live) message.error('读取脚本源码失败') })
    return () => { live = false }
  }, [form, path, script, selected])

  const open = (entry: Script | 'new') => {
    clearRun()
    setInput('{}')
    setTab('source')
    setDirty(false)
    setVersionStatus(entry === 'new' ? null : entry.testStatus ?? null)
    setVersions([])
    setSelected(entry === 'new' ? undefined : entry.revision)
    setSourceReady(entry === 'new')
    form.resetFields()
    form.setFieldsValue(entry === 'new' ? { language: 'javascript' } : { name: entry.name, language: entry.language })
    setOpened(entry)
  }

  const save = async (draft: ScriptDraft) => {
    if (readOnly || !sourceReady || saving) return
    setSaving(true)
    try {
      let parameters
      try { parameters = parseParameters(draft.parameters) }
      catch { message.error('参数默认值必须是合法 JSON'); return }
      const payload = { ...draft, parameters }
      const row = script
        ? await request<Script>('PUT', `${path}/${script.id}`, { ...payload, revision: script.revision })
        : await request<Script>('POST', path, payload)
      setItems(current => [row, ...current.filter(item => item.id !== row.id)])
      setOpened(null)
      message.success('脚本版本已保存')
    } catch (error) {
      message.error(error instanceof ApiError && error.status === 409 ? '脚本名称已存在或版本已变更，请刷新后重试' : '无法保存公共脚本')
    } finally {
      setSaving(false)
    }
  }

  const copyRevision = async (row: Script, revision = row.revision) => {
    const copied = await copyToClipboard(`GET ${path}/${row.id}/revisions/${revision}`)
    if (copied) message.success('已复制固定版本引用')
    else message.error('复制失败')
  }

  const debug = async () => {
    if (!sourceReady || running) return
    let payload: Record<string, unknown>
    try {
      const supplied: unknown = JSON.parse(input)
      if (!supplied || typeof supplied !== 'object' || Array.isArray(supplied)) throw new Error()
      const parameters = parseParameters(form.getFieldValue('parameters'))
      const defaults = Object.fromEntries(parameters.filter(p => 'defaultValue' in p).map(p => [p.name, p.defaultValue]))
      payload = { ...defaults, ...supplied }
      const missing = parameters.find(p => p.required && !Object.prototype.hasOwnProperty.call(payload, p.name))
      if (missing) { message.error(`缺少必填参数 ${missing.name}`); return }
    } catch { message.error('调试输入须为 JSON 对象，参数默认值须为合法 JSON'); return }
    const { language, source } = form.getFieldsValue()
    if (!source?.trim()) { message.error('请输入脚本源码'); return }
    const saved = script && selected != null && !dirty
    const id = ++generation.current
    setTab('result')
    setResult(null)
    setDebugError('')
    setRunning(true)
    try {
      const data = await request<DebugResult>('POST',
        saved ? `${path}/${script.id}/revisions/${selected}/debug` : `${path}/debug`,
        saved ? payload : { language, source, input: payload })
      if (id !== generation.current) return
      setResult({ data, target: saved ? `v${selected}` : '当前草稿' })
      if (saved) {
        const status = data.exitCode === 0 && !data.outputLimited ? 'passed' : 'failed'
        setVersionStatus(status)
        setItems(rows => rows.map(row => row.id === script.id && row.revision === selected ? { ...row, testStatus: status } : row))
      }
    } catch (error) {
      if (id !== generation.current) return
      setDebugError(error instanceof ApiError && error.status === 429 ? '调试执行器繁忙，请稍后重试'
        : error instanceof ApiError && error.status === 403 ? '当前账号没有公共脚本执行权限'
        : '调试运行失败，请检查执行器状态和输入大小')
    } finally {
      if (id === generation.current) setRunning(false)
    }
  }

  const testLabel = (status?: string | null) =>
    status === 'passed' ? <Tag color="green">测试通过</Tag> : status === 'failed' ? <Tag color="red">测试失败</Tag> : <Tag>未测试</Tag>

  const visible = items.filter(row => [row.name, ...(row.tags ?? [])].some(value => value.toLowerCase().includes(search.trim().toLowerCase())))
  if (listError != null) return <PageContainer>
    <PageHeader title="公共脚本" />
    <PageBody>
      {listError instanceof ApiError && listError.status === 404
        ? <AccessDenied title="无法访问此项目的公共脚本" description="请联系项目管理员确认项目仍可访问，并检查你的账号是否已加入项目。" />
        : listError instanceof ApiError && listError.status === 403
          ? <AccessDenied title="没有查看公共脚本的权限" description="请联系管理员授予「公共脚本 · 查询」权限。授权后请重新登录。" />
          : <ReadError error={listError} title="公共脚本暂不可用，请刷新后重试。" />}
    </PageBody>
  </PageContainer>
  return <PageContainer>
    <PageHeader title="公共脚本" extra={<Button type="primary" icon={<PlusOutlined />} onClick={() => open('new')}>新建脚本</Button>} />
    <PageBody>
      <Input.Search allowClear aria-label="搜索公共脚本" placeholder="按名称或标签搜索" style={{ maxWidth: 320, marginBottom: 12 }}
        value={search} onChange={event => setSearch(event.target.value)} />
      <Table<Script> rowKey="id" loading={loading} dataSource={visible} columns={[
        { title: '名称', dataIndex: 'name', render: (name, row) => <Button type="link" onClick={() => open(row)}>{name}</Button> },
        { title: '描述', dataIndex: 'description', ellipsis: true, render: value => value || '—' },
        { title: '标签', dataIndex: 'tags', render: (tags?: string[]) => tags?.map(tag => <Tag key={tag}>{tag}</Tag>) },
        { title: '调试状态', dataIndex: 'testStatus', render: testLabel },
        { title: '语言', dataIndex: 'language', render: (lang: Language) => <Tag>{labels[lang]}</Tag> },
        { title: '版本', dataIndex: 'revision', render: (revision) => `v${revision}` },
        { title: '更新时间', dataIndex: 'updatedAt', render: (time) => new Date(time).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' }) },
        { title: '操作', render: (_, row) => <Space><Button type="link" onClick={() => open(row)}>编辑</Button><Button type="link" onClick={() => void copyRevision(row)}>复制版本引用</Button></Space> },
      ]} />
      <EditDrawer open={opened !== null} width="min(960px, 96vw)" title={script?.name || '新建脚本'}
        okText="保存版本" onCancel={() => { clearRun(); setOpened(null) }} onOk={() => form.submit()} confirmLoading={saving}>
        {script && <Select aria-label="脚本版本" value={selected} options={(versions.length ? versions : [script]).map(v => ({
          value: v.revision, label: `v${v.revision}${v.revision === script.revision ? ' · 当前' : ''}`,
        }))} onChange={revision => {
          clearRun()
          setTab('source')
          setSourceReady(false)
          form.setFieldValue('name', versions.find(v => v.revision === revision)?.name ?? script.name)
          setSelected(revision)
        }} style={{ marginBottom: 16, width: 160 }} />}
        {opened && <Space style={{ marginLeft: script ? 10 : 0, marginBottom: 16 }}>
          {dirty || !script ? <Tag>{result?.target === '当前草稿' ? (result.data.exitCode === 0 && !result.data.outputLimited ? '草稿测试通过' : '草稿测试失败') : '草稿未测试'}</Tag> : testLabel(versionStatus)}
          {script && selected != null && <Button size="small" onClick={() => void copyRevision(script, selected)}>复制版本引用</Button>}
        </Space>}
        <Form form={form} layout="vertical" onFinish={save} onValuesChange={() => { setDirty(true); clearRun() }}>
          <Form.Item label="名称" name="name" rules={[{ required: true, whitespace: true }]}>
            <Input maxLength={120} disabled={readOnly} />
          </Form.Item>
          <Form.Item label="语言" name="language" rules={[{ required: true }]}>
            <Select disabled={!!script} options={Object.entries(labels).map(([value, label]) => ({ value, label }))} />
          </Form.Item>
          <Form.Item label="描述" name="description"><Input.TextArea maxLength={1000} autoSize disabled={readOnly || !sourceReady} /></Form.Item>
          <Form.Item label="标签" name="tags"><Select mode="tags" maxCount={20} disabled={readOnly || !sourceReady} placeholder="输入标签后按回车" /></Form.Item>
          <Collapse size="small" items={[{ key: 'parameters', label: '传递参数', forceRender: true, children:
            <Form.List name="parameters">{(fields, { add, remove }) => <>
              {fields.map(({ key, name }) => <Space key={key} wrap align="start">
                <Form.Item name={[name, 'name']} label="参数名称" rules={[{ required: true, whitespace: true }]}><Input maxLength={64} disabled={readOnly} /></Form.Item>
                <Form.Item name={[name, 'defaultJson']} label="默认值（JSON）"><Input placeholder={'如 "示例" 或 42'} disabled={readOnly} /></Form.Item>
                <Form.Item name={[name, 'description']} label="说明"><Input maxLength={200} disabled={readOnly} /></Form.Item>
                <Form.Item name={[name, 'required']} valuePropName="checked" label="必填"><Checkbox disabled={readOnly} /></Form.Item>
                {!readOnly && <Button style={{ marginTop: 30 }} type="link" onClick={() => remove(name)}>移除</Button>}
              </Space>)}
              {!readOnly && fields.length < 30 && <Button type="dashed" onClick={() => add({})}>添加参数</Button>}
            </>}</Form.List>
          }]} />
          <Form.Item label="调试输入（JSON）" style={{ marginTop: 16 }}>
            <Input.TextArea className="ms-mono" aria-label="调试输入" rows={3} value={input} maxLength={65_536}
              onChange={event => { setInput(event.target.value); clearRun() }} />
          </Form.Item>
          <Tabs activeKey={tab} onChange={setTab} tabBarExtraContent={<Button type="primary" loading={running} disabled={!sourceReady} onClick={() => void debug()}>调试运行</Button>} items={[
            { key: 'source', label: '源码', forceRender: true, children:
              <Form.Item name="source" rules={[{ required: true, whitespace: true }]}>
                <Input.TextArea aria-label="源码" className="ms-mono" autoSize={{ minRows: 12, maxRows: 22 }} maxLength={65_536}
                  readOnly={readOnly || !sourceReady} placeholder={sourceReady ? '粘贴脚本源码' : '正在读取脚本…'} spellCheck={false} />
              </Form.Item> },
            { key: 'result', label: '执行结果', children: <>
              {running && <Alert type="info" title="正在运行脚本" />}
              {debugError && <Alert type="error" title={debugError} />}
              {result && <>
                <Space wrap style={{ marginBottom: 12 }}>
                  <Tag>{result.target}</Tag><Tag>{result.data.durationMs} ms</Tag>
                  <Tag color={result.data.exitCode === 0 ? 'green' : 'red'}>{result.data.timedOut ? '执行超时' : `退出码 ${result.data.exitCode ?? '—'}`}</Tag>
                  {result.data.outputLimited && <Tag color="orange">输出已达上限</Tag>}
                </Space>
                <Form.Item label="标准输出"><Input.TextArea className="ms-mono" readOnly rows={6} value={result.data.stdout} /></Form.Item>
                <Form.Item label="标准错误"><Input.TextArea className="ms-mono" readOnly rows={4} value={result.data.stderr} /></Form.Item>
              </>}
              {!running && !debugError && !result && <Alert type="info" title="暂无本次执行结果" />}
            </> },
          ]} />
        </Form>
      </EditDrawer>
    </PageBody>
  </PageContainer>
}
