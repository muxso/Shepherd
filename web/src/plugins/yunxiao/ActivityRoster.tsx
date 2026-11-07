import { useEffect, useState } from 'react'
import { Alert, Button, Card, Checkbox, Drawer, Form, Input, Select, Space, Spin } from 'antd'
import { activityApi, type ActivityMember, type ActivityMemberSetting } from './api'
export default function ActivityRoster({ sourceId, candidates, open, onClose, onSaved }: { sourceId: string; candidates: ActivityMember[]; open: boolean; onClose: () => void; onSaved: () => void }) {
  const [form] = Form.useForm<{ members: ActivityMemberSetting[] }>()
  const [loading, setLoading] = useState(false), [saving, setSaving] = useState(false), [error, setError] = useState(''), [canUpdate, setCanUpdate] = useState(false)
  useEffect(() => {
    if (!open) return
    let live = true
    setLoading(true); setCanUpdate(false); setError(''); form.resetFields()
    activityApi.members(sourceId).then(data => { if (live) { form.setFieldsValue({ members: data.members }); setCanUpdate(data.permissions.update) } }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [open, sourceId, form])
  const save = async () => {
    const values = await form.validateFields().catch(() => null)
    if (!values) return
    setSaving(true); setError('')
    try { await activityApi.saveMembers(sourceId, values.members || []); onClose(); onSaved() }
    catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setSaving(false) }
  }
  return <Drawer title="统计人员名单" open={open} onClose={() => { if (!saving) onClose() }} size={740} styles={{ wrapper: { maxWidth: '96vw' } }} extra={canUpdate && <Button type="primary" loading={saving} disabled={loading} onClick={save}>保存名单</Button>}>
    <Alert type="info" showIcon title="明确纳入统计的人员，才判断是否无记录" description="支持同一成员选择多个岗位视角。仅在区间采集完整时标记未记录产出；停用成员不参与统计。当前观察到的人员只是候选名单，不能代表组织全员。" style={{ marginBottom: 16 }} />
    {error && <Alert type="error" showIcon title={error} style={{ marginBottom: 16 }} />}
    <Spin spinning={loading}><Form form={form} layout="vertical" disabled={!canUpdate || saving} initialValues={{ members: [] }}>
      <Form.List name="members">{(fields, { add, remove }) => <>
        {fields.map(({ key, name, ...rest }) => <Card key={key} size="small" title={`成员 ${name + 1}`} extra={canUpdate && <Button type="text" danger onClick={() => remove(name)}>移除</Button>} style={{ marginBottom: 12 }}>
          <div className="yx-activity-form-row">
            <Form.Item {...rest} name={[name, 'name']} label="成员姓名" rules={[{ required: true, whitespace: true, message: '填写真实成员姓名' }]}><Input list="yx-activity-member-candidates" maxLength={85} placeholder="真实姓名或统一后的账号名" /></Form.Item>
            <Form.Item {...rest} name={[name, 'group']} label="分组（可选）"><Input maxLength={85} placeholder="使用真实分组" /></Form.Item>
          </div>
          <Space align="start" wrap>
            <Form.Item {...rest} name={[name, 'roles']} label="参与视角" rules={[{ required: true, type: 'array', min: 1, message: '至少选择一个视角' }]}><Select mode="multiple" style={{ minWidth: 260 }} options={[{ value: 'code', label: '研发 · 代码' }, { value: 'product', label: '产品 · 需求' }, { value: 'test', label: '测试 · 质量' }]} /></Form.Item>
            <Form.Item {...rest} name={[name, 'enabled']} valuePropName="checked" label="统计状态"><Checkbox>纳入统计</Checkbox></Form.Item>
          </Space>
        </Card>)}
        <datalist id="yx-activity-member-candidates">{candidates.map(p => <option key={p.name} value={p.name} />)}</datalist>
        {canUpdate && <Button type="dashed" block onClick={() => add({ name: '', group: '', roles: [], enabled: true })}>添加统计人员</Button>}
        {!fields.length && !loading && <p className="yx-muted">尚未配置统计名单；不会把未知成员判为没有产出。</p>}
      </>}</Form.List>
    </Form></Spin>
  </Drawer>
}
