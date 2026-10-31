import { useState } from 'react'
import { Alert, Button, Input, Modal, Space, Typography } from 'antd'
import { environmentPluginApi as api, type PluginOutput, type Profile, type SshReport } from './api'
import { downloadReport } from './reports'

export function SshCommand({ environmentId, profile, onClose }: { environmentId: string; profile: Profile; onClose: () => void }) {
  const [command, setCommand] = useState('printf "ok\\n"')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [result, setResult] = useState<PluginOutput<SshReport> | null>(null)
  async function run() {
    setBusy(true); setError(''); setResult(null)
    try { setResult(await api.execute<SshReport>(environmentId, profile.id, { command })) }
    catch (error) { setError(error instanceof Error ? error.message : String(error)) }
    finally { setBusy(false) }
  }
  return <Modal title={`SSH 命令 · ${profile.name}`} open width={800} maskClosable={!busy} closable={!busy} onCancel={onClose} footer={<Space>
    <Button aria-label="关闭 SSH 命令" disabled={busy} onClick={onClose}>关闭</Button>
    <Button disabled={!result || busy} onClick={() => result && downloadReport(`ssh-report-${profile.id}.json`, result)}>导出报告</Button>
    <Button type="primary" aria-label="执行 SSH 命令" loading={busy} disabled={!command.trim()} onClick={() => void run()}>执行命令</Button>
  </Space>}>
    <Space direction="vertical" style={{ width: '100%' }}>
      <label style={{ width: '100%' }}>远端命令<Input.TextArea aria-label="SSH 远端命令" rows={5} disabled={busy} value={command} onChange={event => setCommand(event.target.value)} /></label>
      <Typography.Text type="secondary">使用保存的认证和主机指纹。超时以配置和系统限额中较小值为准；关闭连接不保证停止已启动的远端进程。</Typography.Text>
      {error && <Alert type="error" message={error} />}
      {result && <>
        <Alert type={result.status === 'success' ? 'success' : 'error'} message={result.status === 'success' ? 'SSH 执行通过' : 'SSH 命令未成功完成'} />
        <Typography.Text>耗时 {result.data.elapsedMs} ms · 主机指纹 {result.data.hostKeySha256}</Typography.Text>
        {result.data.termination.kind === 'exit' ? <Typography.Text>退出码 {result.data.termination.code}</Typography.Text> : result.data.termination.kind === 'signal' ? <Alert type="error" message={`信号终止：${result.data.termination.name}`} description={result.data.termination.message} /> : <Alert type="error" message="未收到退出状态" />}
        <Typography.Text strong>stdout ({result.data.stdout.encoding})</Typography.Text><pre style={{ maxHeight: 250, overflow: 'auto', whiteSpace: 'pre-wrap' }}>{result.data.stdout.value}</pre>
        <Typography.Text strong>stderr ({result.data.stderr.encoding})</Typography.Text><pre style={{ maxHeight: 250, overflow: 'auto', whiteSpace: 'pre-wrap' }}>{result.data.stderr.value}</pre>
      </>}
    </Space>
  </Modal>
}
