import { useState } from 'react'
import { Alert, Button, Input, Modal, Space, Typography } from 'antd'
import { environmentPluginApi as api, type PluginOutput, type Profile, type RedisReport } from './api'
import { downloadReport } from './reports'

function array(source: string, name: string): unknown[] {
  const value: unknown = JSON.parse(source || '[]')
  if (!Array.isArray(value)) throw new Error(`${name}必须是 JSON 数组`)
  return value
}
export function RedisCommand({ environmentId, profile, onClose }: { environmentId: string; profile: Profile; onClose: () => void }) {
  const [command, setCommand] = useState('PING')
  const [argumentsJson, setArguments] = useState('[]')
  const [extract, setExtract] = useState('[{"variable":"value","pointer":""}]')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [result, setResult] = useState<PluginOutput<RedisReport> | null>(null)
  async function run() {
    setBusy(true); setError(''); setResult(null)
    try { setResult(await api.execute<RedisReport>(environmentId, profile.id, { command, arguments: array(argumentsJson, '参数'), extract: array(extract, '提取规则') })) }
    catch (error) { setError(error instanceof Error ? error.message : String(error)) }
    finally { setBusy(false) }
  }
  return <Modal title={`Redis 命令 · ${profile.name}`} open width={800} maskClosable={!busy} closable={!busy} onCancel={onClose} footer={<Space>
    <Button aria-label="关闭 Redis 命令" disabled={busy} onClick={onClose}>关闭</Button>
    <Button disabled={!result || busy} onClick={() => result && downloadReport(`redis-report-${profile.id}.json`, result)}>导出报告</Button>
    <Button type="primary" aria-label="执行命令" loading={busy} disabled={!command.trim()} onClick={() => void run()}>执行命令</Button>
  </Space>}>
    <Space direction="vertical" style={{ width: '100%' }}>
      <label style={{ width: '100%' }}>命令<Input aria-label="Redis 命令名称" disabled={busy} value={command} onChange={event => setCommand(event.target.value)} /></label>
      <label style={{ width: '100%' }}>参数（JSON 数组）<Input.TextArea aria-label="Redis 命令参数" rows={3} disabled={busy} value={argumentsJson} onChange={event => setArguments(event.target.value)} placeholder={'["key with spaces", {"hex":"00ff"}]'} /></label>
      <Typography.Text type="secondary">每个数组项是一个完整参数，支持字符串和十六进制二进制对象。认证、数据库和协议使用保存的配置。</Typography.Text>
      <label style={{ width: '100%' }}>提取规则（JSON 数组）<Input.TextArea aria-label="Redis 提取规则" rows={3} disabled={busy} value={extract} onChange={event => setExtract(event.target.value)} /></label>
      <Typography.Text type="secondary">pointer 使用 JSON Pointer：空字符串取整个结果，/0 取数组首项。required 默认启用；空值为 null，缺少路径会标记失败。变量随本次报告返回。</Typography.Text>
      {error && <Alert type="error" message={error} />}
      {result && <>
        <Alert type={result.status === 'success' ? 'success' : 'error'} message={result.status === 'success' ? 'Redis 执行通过' : 'Redis 结果提取未通过'} />
        <Typography.Text>结果类型 {result.data.kind} · 耗时 {result.data.elapsedMs} ms</Typography.Text>
        <pre style={{ maxHeight: 320, overflow: 'auto', whiteSpace: 'pre-wrap' }}>{JSON.stringify(result.data.value, null, 2)}</pre>
        {result.assertions.filter(assertion => !assertion.passed).map(assertion => <Alert type="error" key={assertion.id} message={`${assertion.id}: ${assertion.message}`} />)}
        {Object.keys(result.variables).length > 0 && <><Typography.Text strong>提取变量</Typography.Text><pre>{JSON.stringify(result.variables, null, 2)}</pre></>}
      </>}
    </Space>
  </Modal>
}
