import { useState } from 'react'
import { Alert, Button, Input, InputNumber, Modal, Select, Space, Table, Typography } from 'antd'
import { environmentPluginApi as api, type DatabaseReport, type PluginOutput, type Profile } from './api'

function jsonArray(source: string, label: string): unknown[] {
  const value: unknown = JSON.parse(source || '[]')
  if (!Array.isArray(value)) throw new Error(`${label}必须是 JSON 数组`)
  return value
}
const cell = (value: unknown) => value === null ? 'NULL' : typeof value === 'string' ? value : JSON.stringify(value)

export function DatabaseQuery({ environmentId, profile, onClose }: { environmentId: string; profile: Profile; onClose: () => void }) {
  const [query, setQuery] = useState('SELECT 1 AS value')
  const [mode, setMode] = useState<'query' | 'execute'>('query')
  const [parameters, setParameters] = useState('[]')
  const [extract, setExtract] = useState('[]')
  const [maxRows, setMaxRows] = useState(100)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [result, setResult] = useState<PluginOutput<DatabaseReport> | null>(null)
  async function run() {
    setBusy(true); setError(''); setResult(null)
    try {
      setResult(await api.execute(environmentId, profile.id, { query, mode, parameters: jsonArray(parameters, '参数'), extract: mode === 'query' ? jsonArray(extract, '提取规则') : [], maxRows }))
    } catch (error) { setError(error instanceof Error ? error.message : String(error)) }
    finally { setBusy(false) }
  }
  function download() {
    if (!result) return
    const url = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' }))
    const link = document.createElement('a')
    link.href = url; link.download = `database-report-${profile.id}.json`; link.click()
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }
  return <Modal title={`数据库查询 · ${profile.name}`} open width={960} maskClosable={!busy} closable={!busy} onCancel={onClose} footer={<Space>
    <Button aria-label="关闭查询" disabled={busy} onClick={onClose}>关闭</Button>
    <Button disabled={!result || busy} onClick={download}>导出报告</Button>
    <Button type="primary" loading={busy} disabled={!query.trim()} onClick={() => void run()}>{mode === 'query' ? '运行查询' : '执行语句'}</Button>
  </Space>}>
    <Space direction="vertical" style={{ width: '100%' }}>
      <Space wrap><Select aria-label="执行模式" disabled={busy} value={mode} onChange={setMode} style={{ width: 200 }} options={[{ value: 'query', label: '查询（只读事务）' }, { value: 'execute', label: '执行（提交写入）' }]} />
        {mode === 'query' && <label>最多返回行数<InputNumber aria-label="最多返回行数" min={1} max={1000} value={maxRows} disabled={busy} onChange={value => setMaxRows(value ?? 100)} /></label>}
      </Space>
      <label style={{ width: '100%' }}>SQL<Input.TextArea aria-label="SQL" rows={5} value={query} disabled={busy} onChange={event => setQuery(event.target.value)} /></label>
      <Typography.Text type="secondary">{profile.pluginId === 'postgres' ? '参数占位符使用 $1、$2。' : '参数占位符使用 ?。'}参数通过绑定传递，支持 text、integer、float、boolean；value 为 null 时保留参数类型。</Typography.Text>
      <label style={{ width: '100%' }}>参数（JSON 数组）<Input.TextArea aria-label="查询参数" rows={2} value={parameters} disabled={busy} placeholder={'[{"type":"integer","value":42}]'} onChange={event => setParameters(event.target.value)} /></label>
      {mode === 'query' && <><label style={{ width: '100%' }}>提取规则（JSON 数组）<Input.TextArea aria-label="结果提取规则" rows={2} value={extract} disabled={busy} placeholder={'[{"variable":"id","row":0,"column":0,"required":true}]'} onChange={event => setExtract(event.target.value)} /></label><Typography.Text type="secondary">行、列序号从 0 开始，也可用唯一列名；required 默认启用，缺少结果会标记失败。提取变量随本次报告返回。</Typography.Text></>}
      {error && <Alert type="error" message={error} />}
      {result && <>
        <Alert type={result.status === 'success' ? 'success' : 'error'} message={result.status === 'success' ? '数据库执行通过' : '结果提取未通过'} />
        <Typography.Text>耗时 {result.data.elapsedMs} ms · 返回 {result.data.rows.length} 行 · 影响 {result.data.rowsAffected} 行</Typography.Text>
        {result.data.truncated && <Alert type="warning" message="结果已达到行数上限，仅展示部分结果" />}
        <Table size="small" scroll={{ x: true }} pagination={false} dataSource={result.data.rows.map((values, index) => ({ key: index, values }))} columns={result.data.columns.map((column, index) => ({ key: index, title: <span>{column.name}<Typography.Text type="secondary"> ({column.sqlType})</Typography.Text></span>, render: (_: unknown, row: { values: unknown[] }) => cell(row.values[index]) }))} />
        {result.assertions.filter(assertion => !assertion.passed).map(assertion => <Alert key={assertion.id} type="error" message={`${assertion.id}: ${assertion.message}`} />)}
        {Object.keys(result.variables).length > 0 && <><Typography.Text strong>提取变量</Typography.Text><pre>{JSON.stringify(result.variables, null, 2)}</pre></>}
      </>}
    </Space>
  </Modal>
}
