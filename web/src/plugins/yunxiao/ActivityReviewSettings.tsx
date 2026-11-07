import { useEffect, useState } from 'react'
import { Alert, Button, Card, Drawer, Form, InputNumber, Spin, Switch } from 'antd'
import { activityApi, type ActivityReviewPolicy } from './api'
export default function ActivityReviewSettings({ sourceId, open, onClose, onSaved }: { sourceId: string; open: boolean; onClose: () => void; onSaved: () => void }) {
  const [form] = Form.useForm<ActivityReviewPolicy>()
  const [loading, setLoading] = useState(false), [saving, setSaving] = useState(false), [canUpdate, setCanUpdate] = useState(false), [error, setError] = useState('')
  useEffect(() => {
    if (!open) return
    let live = true; setLoading(true); setCanUpdate(false); setError(''); form.resetFields()
    activityApi.reviewPolicy(sourceId).then(value => { if (live) { form.setFieldsValue(value); setCanUpdate(value.permissions.update) } }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [sourceId, open, form])
  const save = async () => {
    const values = await form.validateFields().catch(() => null)
    if (!values) return
    setSaving(true); setError('')
    try { await activityApi.saveReviewPolicy(sourceId, values); onClose(); onSaved() } catch (e) { setError(e instanceof Error ? e.message : String(e)) } finally { setSaving(false) }
  }
  return <Drawer title="产出记录核验规则" open={open} onClose={onClose} size={680} styles={{ wrapper: { maxWidth: '96vw' } }} extra={canUpdate && <Button type="primary" loading={saving} disabled={loading} onClick={save}>保存规则</Button>}>
    <p className="yx-muted">规则按岗位保存，统一应用到该数据源的成员。仅在区间采集完整时提示“需核验”，并展示原始依据；由负责人结合实际需求、请假及岗位安排核验。</p>
    {error && <Alert type="error" showIcon title={error} />}
    <Spin spinning={loading}><Form form={form} layout="vertical" disabled={!canUpdate || saving}>{(['code', 'product', 'test'] as const).map((role, i) => <Card size="small" key={role} title={['研发 · 代码', '产品 · 需求', '测试 · 质量'][i]} style={{ marginBottom: 12 }}>
      <Form.Item name={[role, 'enabled']} label="启用记录核验" valuePropName="checked"><Switch /></Form.Item>
      <div className="yx-activity-policy-fields">
        <Form.Item name={[role, 'inactiveDays']} label="距最近活动天数" rules={[{ required: true }]} extra="0 为关闭此项"><InputNumber min={0} max={365} precision={0} /></Form.Item>
        <Form.Item name={[role, 'activeWindowDays']} label="活跃日观察区间" rules={[{ required: true }]}><InputNumber min={1} max={365} precision={0} /></Form.Item>
        <Form.Item name={[role, 'minActiveDays']} label="区间最少活跃天数" rules={[{ required: true }]} extra="0 为关闭此项"><InputNumber min={0} max={365} precision={0} /></Form.Item>
      </div>
    </Card>)}</Form></Spin>
  </Drawer>
}
