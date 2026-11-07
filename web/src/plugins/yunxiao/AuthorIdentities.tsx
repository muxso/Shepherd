import { useEffect, useState } from 'react'
import { Alert, Button, Card, Drawer, Form, Input, Select, Space, Spin } from 'antd'
import { devopsApi, type AuthorIdentity, type CodeCommit } from './api'

export default function AuthorIdentities({ sourceId, commits, onSaved, open: controlledOpen, onOpenChange }: { sourceId: string; commits: Pick<CodeCommit, 'author' | 'authorRaw'>[]; onSaved: () => void; open?: boolean; onOpenChange?: (open: boolean) => void }) {
  const [localOpen, setLocalOpen] = useState(false), [loading, setLoading] = useState(false), [saving, setSaving] = useState(false)
  const open = controlledOpen ?? localOpen
  const setOpen = (value: boolean) => { setLocalOpen(value); onOpenChange?.(value) }
  const [error, setError] = useState(''), [canUpdate, setCanUpdate] = useState(false)
  const [form] = Form.useForm<{ identities: AuthorIdentity[] }>()
  useEffect(() => {
    if (!open) return
    let live = true
    setLoading(true); setError(''); setCanUpdate(false); form.resetFields()
    devopsApi.authorIdentities(sourceId).then(data => {
      if (live) { form.setFieldsValue({ identities: data.identities }); setCanUpdate(data.permissions.update) }
    }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [sourceId, open, form])
  const save = async () => {
    const values = await form.validateFields().catch(() => null)
    if (!values) return
    setSaving(true); setError('')
    try { await devopsApi.setAuthorIdentities(sourceId, values.identities || []); setOpen(false); onSaved() }
    catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setSaving(false) }
  }
  const options = [...new Set(commits.flatMap(c => [c.authorRaw, c.author]).filter((v): v is string => !!v))].sort().map(value => ({ value, label: value }))
  return <>
    {controlledOpen === undefined && <Button onClick={() => setOpen(true)}>账号归并管理</Button>}
    <Drawer title="提交账号归并管理" open={open} onClose={() => { if (!saving) setOpen(false) }} size={700} styles={{ wrapper: { maxWidth: '96vw' } }} extra={canUpdate && <Button type="primary" loading={saving} disabled={loading} onClick={save}>保存配置</Button>}>
      <Alert type="info" showIcon title="一个人可以关联多个提交账号" description="配置仅对当前数据源生效，保存后统一更新历史和后续统计。姓名自动作为匹配账号；别名区分大小写且不能分配给多人。删除配置可恢复原始归属。" style={{ marginBottom: 16 }} />
      {error && <Alert type="error" showIcon title={error} style={{ marginBottom: 16 }} />}
      <Spin spinning={loading}>
        {!loading && !canUpdate && !error && <Alert type="info" title="当前账号可查看配置；修改需要项目更新权限。" style={{ marginBottom: 16 }} />}
        <Form form={form} layout="vertical" disabled={!canUpdate || saving} initialValues={{ identities: [] }}>
          <Form.List name="identities">{(fields, { add, remove }) => <>
            {fields.map(({ key, name, ...rest }) => <Card key={key} size="small" title={`人员 ${name + 1}`} extra={canUpdate && <Button type="text" danger disabled={saving} onClick={() => remove(name)}>移除此配置</Button>} style={{ marginBottom: 12 }}>
              <div className="yx-identity-fields">
                <Form.Item {...rest} name={[name, 'name']} label="统一姓名" rules={[{ required: true, whitespace: true, message: '请填写统一姓名' }]}><Input maxLength={85} placeholder="统计中显示的姓名" /></Form.Item>
                <Form.Item {...rest} name={[name, 'group']} label="所属分组（可选）"><Input maxLength={85} placeholder="留空使用姓名已有的唯一分组" /></Form.Item>
              </div>
              <Form.Item {...rest} name={[name, 'aliases']} label="其他提交账号" style={{ marginBottom: 0 }}><Select mode="tags" options={options} placeholder="选择或输入账号，回车添加多个" tokenSeparators={[',', '，']} /></Form.Item>
            </Card>)}
            {canUpdate && <Button type="dashed" block disabled={saving} onClick={() => add({ name: '', group: '', aliases: [] })}>添加人员归并</Button>}
            {!fields.length && !loading && <p className="yx-muted">尚未配置账号归并。</p>}
          </>}</Form.List>
        </Form>
      </Spin>
      <Space direction="vertical" style={{ marginTop: 20 }}><span className="yx-muted">分组留空时，从统一姓名的原始提交取唯一已知分组；没有唯一分组时显示待归属。原始提交账号保留在明细中。</span></Space>
    </Drawer>
  </>
}
