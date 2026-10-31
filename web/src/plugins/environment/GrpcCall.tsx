import { useEffect, useState } from 'react'
import { Alert, Button, Input, Modal, Select, Space, Spin, Typography } from 'antd'
import { environmentPluginApi as api, type GrpcMethod, type GrpcReport, type PluginOutput, type Profile } from './api'
import { downloadReport } from './reports'

export function GrpcCall({ environmentId, profile, onClose }: { environmentId: string; profile: Profile; onClose: () => void }) {
  const [methods, setMethods] = useState<GrpcMethod[]>([])
  const [method, setMethod] = useState('')
  const [message, setMessage] = useState('{}')
  const [metadata, setMetadata] = useState('{}')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [result, setResult] = useState<PluginOutput<GrpcReport> | null>(null)
  useEffect(() => {
    let active = true
    void api.metadata<{ methods: GrpcMethod[] }>(environmentId, profile.id).then(response => {
      if (!active) return
      setMethods(response.metadata.methods)
      const first = response.metadata.methods.find(m => !m.clientStreaming && !m.serverStreaming)
      if (first) { setMethod(first.path); setMessage(JSON.stringify(first.inputExample ?? {}, null, 2)) }
    }).catch(error => { if (active) setError(error instanceof Error ? error.message : String(error)) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [environmentId, profile.id])
  async function run() {
    setBusy(true); setError(''); setResult(null)
    try { setResult(await api.execute<GrpcReport>(environmentId, profile.id, { method, message: JSON.parse(message), metadata: JSON.parse(metadata) })) }
    catch (error) { setError(error instanceof Error ? error.message : String(error)) }
    finally { setBusy(false) }
  }
  return <Modal title={`gRPC 调用 · ${profile.name}`} open width={850} maskClosable={!busy} closable={!busy} onCancel={onClose} footer={<Space>
    <Button disabled={busy} onClick={onClose}>关闭</Button>
    <Button disabled={!result || busy} onClick={() => result && downloadReport(`grpc-report-${profile.id}.json`, result)}>导出报告</Button>
    <Button type="primary" aria-label="执行 gRPC 调用" loading={busy} disabled={loading || !method} onClick={() => void run()}>执行调用</Button>
  </Space>}>
    <Space direction="vertical" style={{ width: '100%' }}>
      <Typography.Text type="secondary">方法和请求示例来自保存的 Proto 定义。支持一元调用；连接测试仅验证传输和 TLS，调用结果以 gRPC 状态码为准。</Typography.Text>
      {loading && <Spin />}
      <label style={{ width: '100%' }}>方法<Select aria-label="gRPC 方法" style={{ width: '100%' }} disabled={loading || busy} value={method || undefined} options={methods.map(m => ({ value: m.path, label: `${m.path}${m.clientStreaming || m.serverStreaming ? '（流式，暂不支持）' : ''}`, disabled: m.clientStreaming || m.serverStreaming }))} onChange={path => { setMethod(path); setMessage(JSON.stringify(methods.find(m => m.path === path)?.inputExample ?? {}, null, 2)); setResult(null) }} /></label>
      <label style={{ width: '100%' }}>请求（Protobuf JSON）<Input.TextArea aria-label="gRPC 请求" rows={7} disabled={busy} value={message} onChange={event => setMessage(event.target.value)} /></label>
      <label style={{ width: '100%' }}>公开 Metadata（JSON）<Input.TextArea aria-label="gRPC Metadata" rows={3} disabled={busy} value={metadata} onChange={event => setMetadata(event.target.value)} /></label>
      <Typography.Text type="secondary">认证信息使用配置中的凭据引用。二进制 Metadata 的名称以 -bin 结尾，值使用 Base64。</Typography.Text>
      {error && <Alert type="error" message={error} />}
      {result && <>
        <Alert type={result.status === 'success' ? 'success' : 'error'} message={`gRPC 状态 ${result.data.status.code} · ${result.data.status.message}`} />
        <Typography.Text>耗时 {result.data.elapsedMs} ms</Typography.Text>
        <Typography.Text strong>响应</Typography.Text><pre style={{ maxHeight: 300, overflow: 'auto' }}>{JSON.stringify(result.data.response, null, 2)}</pre>
        <Typography.Text strong>Metadata / 状态详情</Typography.Text><pre style={{ maxHeight: 200, overflow: 'auto' }}>{JSON.stringify({ metadata: result.data.metadata, details: result.data.status.details }, null, 2)}</pre>
      </>}
    </Space>
  </Modal>
}
