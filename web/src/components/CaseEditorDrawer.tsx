import { useEffect, useState } from 'react'
import { Button, Card, Drawer, Form, Input, Select, Space, Table, Tabs, Tag, Typography } from 'antd'
import { message } from '../feedback'
import { SendOutlined, PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiDefinition, type DebugResponse } from '../api'
import { methodColor } from './tags'
import AssertionEditor, { type Assertion } from './AssertionEditor'
import { useI18n } from '../i18n'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
type HeaderRow = { key: string; value: string }

// MeterSphere 风接口用例工作台:一处完成 组装请求 → 发送调试 → 看响应 → 配断言 → 保存为用例。
export default function CaseEditorDrawer({
  open,
  definition,
  onClose,
  onSaved,
}: {
  open: boolean
  definition: ApiDefinition
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useI18n()
  const [name, setName] = useState('')
  const [method, setMethod] = useState(definition.method || 'GET')
  const [url, setUrl] = useState(definition.path || '')
  const [headers, setHeaders] = useState<HeaderRow[]>([{ key: '', value: '' }])
  const [body, setBody] = useState('')
  const [assertions, setAssertions] = useState<Assertion[]>([{ type: 'StatusIs', args: 200 }])
  const [sending, setSending] = useState(false)
  const [saving, setSaving] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [err, setErr] = useState('')

  // 每次打开按当前接口定义重置。
  useEffect(() => {
    if (open) {
      setName('')
      setMethod(definition.method || 'GET')
      setUrl(definition.path || '')
      setHeaders([{ key: '', value: '' }])
      setBody('')
      setAssertions([{ type: 'StatusIs', args: 200 }])
      setResp(null)
      setErr('')
    }
  }, [open, definition])

  const setHeader = (i: number, patch: Partial<HeaderRow>) =>
    setHeaders((hs) => hs.map((h, idx) => (idx === i ? { ...h, ...patch } : h)))

  const send = async () => {
    if (!url.trim()) return message.warning(t('case.urlRequired', '请输入 URL'))
    setSending(true)
    setErr('')
    setResp(null)
    try {
      setResp(await api.debugSend({ method, url, headers: headers.filter((h) => h.key.trim()), body: body.trim() || undefined }))
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : t('case.sendFailed', '发送失败'))
    } finally {
      setSending(false)
    }
  }

  const save = async () => {
    if (!name.trim()) return message.warning(t('case.nameRequired', '请填用例名'))
    if (!url.trim()) return message.warning(t('case.urlRequiredSave', '请填 URL'))
    setSaving(true)
    try {
      await api.createCase(definition.id, { name: name.trim(), method, url, body: body.trim() || undefined, assertions })
      message.success(t('case.saved', '用例已保存'))
      onSaved()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('case.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Drawer
      title={`${t('case.newCase', '新建用例')} · ${definition.name}`}
      open={open}
      onClose={onClose}
      width="68%"
      styles={{ body: { paddingTop: 12 } }}
      footer={
        <div style={{ textAlign: 'right' }}>
          <Space>
            <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
            <Button type="primary" loading={saving} onClick={save}>{t('case.saveCase', '保存用例')}</Button>
          </Space>
        </div>
      }
    >
      <Form layout="vertical">
        <Form.Item label={t('case.name', '用例名')} required style={{ marginBottom: 12 }}>
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('case.namePlaceholder', '如:登录成功')} />
        </Form.Item>
      </Form>

      {/* 请求行 */}
      <Space.Compact style={{ width: '100%', marginBottom: 12 }}>
        <Select value={method} onChange={setMethod} style={{ width: 110 }} options={METHODS.map((m) => ({ value: m, label: m }))} />
        <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="http://127.0.0.1:9180/healthz" className="ms-mono" onPressEnter={send} />
        <Button type="primary" icon={<SendOutlined />} loading={sending} onClick={send}>{t('a.send', '发送')}</Button>
      </Space.Compact>

      <Tabs
        size="small"
        items={[
          {
            key: 'headers',
            label: 'Headers',
            children: (
              <Space direction="vertical" style={{ width: '100%' }}>
                {headers.map((h, i) => (
                  <Space.Compact key={i} style={{ width: '100%' }}>
                    <Input placeholder={t('case.headerName', '名')} value={h.key} onChange={(e) => setHeader(i, { key: e.target.value })} style={{ width: 240 }} className="ms-mono" />
                    <Input placeholder={t('case.headerValue', '值')} value={h.value} onChange={(e) => setHeader(i, { value: e.target.value })} />
                    <Button icon={<DeleteOutlined />} onClick={() => setHeaders((hs) => hs.filter((_, idx) => idx !== i))} />
                  </Space.Compact>
                ))}
                <Button icon={<PlusOutlined />} size="small" onClick={() => setHeaders((hs) => [...hs, { key: '', value: '' }])}>{t('case.addRow', '加一行')}</Button>
              </Space>
            ),
          },
          {
            key: 'body',
            label: 'Body',
            children: <Input.TextArea rows={6} value={body} onChange={(e) => setBody(e.target.value)} placeholder='{"username":"admin"}' className="ms-mono" />,
          },
          {
            key: 'assert',
            label: `${t('case.assertions', '断言')} (${assertions.length})`,
            children: <AssertionEditor value={assertions} onChange={setAssertions} />,
          },
        ]}
      />

      {(resp || err) && (
        <Card
          size="small"
          style={{ marginTop: 12 }}
          title={
            <Space>
              {t('case.response', '响应')}
              {resp && <Tag color={resp.status < 400 ? 'green' : 'red'}>{resp.status}</Tag>}
              {resp && <Typography.Text type="secondary" style={{ fontSize: 12 }}>{resp.latencyMs} ms</Typography.Text>}
              <Tag color={methodColor(method)}>{method}</Tag>
            </Space>
          }
        >
          {err ? (
            <Typography.Text type="danger">{err}</Typography.Text>
          ) : (
            <Tabs
              size="small"
              items={[
                {
                  key: 'rbody',
                  label: 'Body',
                  children: (
                    <pre style={{ background: '#0f1419', color: '#d6deeb', padding: 12, borderRadius: 6, maxHeight: 280, overflow: 'auto', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                      {resp?.body || t('case.emptyBody', '(空)')}
                    </pre>
                  ),
                },
                {
                  key: 'rheaders',
                  label: `Headers (${resp?.headers.length || 0})`,
                  children: (
                    <Table
                      size="small"
                      pagination={false}
                      rowKey={(_, i) => String(i)}
                      dataSource={(resp?.headers || []).map(([k, v]) => ({ k, v }))}
                      columns={[
                        { title: t('case.headerName', '名'), dataIndex: 'k', width: 220 },
                        { title: t('case.headerValue', '值'), dataIndex: 'v', render: (v: string) => <span className="ms-mono">{v}</span> },
                      ]}
                    />
                  ),
                },
              ]}
            />
          )}
        </Card>
      )}
    </Drawer>
  )
}
