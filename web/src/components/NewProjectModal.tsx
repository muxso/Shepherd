import { useEffect, useState } from 'react'
import { Modal, Form, Input, Select, Space, Button, Divider } from 'antd'
import { message } from '../feedback'
import { api, ApiError, type Organization } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

// 新建项目(并可顺手新建组织)——首装无项目时让 UI 自助可用。
export default function NewProjectModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { reloadProjects, setProjectId } = useApp()
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [orgs, setOrgs] = useState<Organization[]>([])
  const [saving, setSaving] = useState(false)
  const [newOrg, setNewOrg] = useState('')

  const loadOrgs = async () => {
    try {
      setOrgs((await api.organizations()).items)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('proj.loadOrgsFail', '加载组织失败'))
    }
  }

  useEffect(() => {
    if (open) loadOrgs()
  }, [open])

  const addOrg = async () => {
    if (!newOrg.trim()) return
    try {
      const o = await api.createOrganization(newOrg.trim())
      setNewOrg('')
      await loadOrgs()
      form.setFieldValue('organizationId', o.id)
      message.success(t('proj.orgCreated', '组织已创建'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('proj.createOrgFail', '创建组织失败'))
    }
  }

  return (
    <Modal title={t('proj.title', '新建项目')} open={open} onCancel={onClose} onOk={() => form.submit()} confirmLoading={saving} destroyOnHidden>
      <Form
        form={form}
        layout="vertical"
        preserve={false}
        onFinish={async (v: { organizationId: string; name: string }) => {
          setSaving(true)
          try {
            const p = await api.createProject(v.organizationId, v.name)
            message.success(t('proj.projCreated', '项目已创建'))
            await reloadProjects()
            setProjectId(p.id)
            onClose()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('proj.createFail', '创建失败'))
          } finally {
            setSaving(false)
          }
        }}
      >
        <Form.Item name="organizationId" label={t('proj.org', '所属组织')} rules={[{ required: true, message: t('proj.orgRequired', '请选择组织') }]}>
          <Select
            placeholder={t('proj.orgPlaceholder', '选择组织')}
            options={orgs.map((o) => ({ value: o.id, label: o.name }))}
            popupRender={(menu) => (
              <>
                {menu}
                <Divider style={{ margin: '8px 0' }} />
                <Space style={{ padding: '0 8px 4px' }}>
                  <Input
                    placeholder={t('proj.newOrgPlaceholder', '新组织名')}
                    value={newOrg}
                    onChange={(e) => setNewOrg(e.target.value)}
                    onKeyDown={(e) => e.stopPropagation()}
                  />
                  <Button type="link" onClick={addOrg}>
                    {t('proj.newOrg', '新建组织')}
                  </Button>
                </Space>
              </>
            )}
          />
        </Form.Item>
        <Form.Item name="name" label={t('proj.name', '项目名')} rules={[{ required: true }]}>
          <Input placeholder={t('proj.namePlaceholder', '如:电商核心')} />
        </Form.Item>
      </Form>
    </Modal>
  )
}
