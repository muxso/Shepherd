import { useEffect, useMemo, useState } from 'react'
import { Button, Card, Drawer, Empty, Input, Segmented, Select, Space, Table, Tag, Upload, message } from 'antd'
import { InboxOutlined, SearchOutlined, FolderOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type ProjectFile } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

// 文件管理(项目模块子标签,对齐参考图 #49/#50)。左侧文件树(简化)+ 右侧列表 + 添加文件抽屉。
export default function FileManagement() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [files, setFiles] = useState<ProjectFile[]>([])
  const [loading, setLoading] = useState(false)
  const [q, setQ] = useState('')
  const [fmt, setFmt] = useState('all')
  const [addOpen, setAddOpen] = useState(false)

  const load = () => {
    if (!projectId) { setFiles([]); return }
    setLoading(true)
    api.projectFiles(projectId).then((f) => setFiles(Array.isArray(f) ? f : [])).catch(() => setFiles([])).finally(() => setLoading(false))
  }
  useEffect(load, [projectId]) // eslint-disable-line react-hooks/exhaustive-deps

  const formats = useMemo(() => Array.from(new Set(files.map((f) => f.fileFormat).filter(Boolean))), [files])
  const rows = files.filter((f) => (!q || f.name.toLowerCase().includes(q.toLowerCase())) && (fmt === 'all' || f.fileFormat === fmt))

  const human = (n: number) => (n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(2)} KB` : `${(n / 1024 / 1024).toFixed(2)} MB`)

  const download = async (f: ProjectFile) => {
    try {
      const d = await api.downloadProjectFile(f.id)
      const a = document.createElement('a')
      a.href = `data:application/octet-stream;base64,${d.contentBase64}`
      a.download = d.name
      a.click()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('file.downloadFailed', '下载失败'))
    }
  }
  const remove = async (f: ProjectFile) => {
    try {
      await api.deleteProjectFile(f.id)
      message.success(t('file.deleted', '已删除'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('file.deleteFailed', '删除失败'))
    }
  }

  const cols: ColumnsType<ProjectFile> = [
    { title: t('file.alias', '文件别名'), dataIndex: 'name', render: (v: string) => <span style={{ color: '#06a561' }}>{v}</span> },
    { title: t('file.format', '文件格式'), dataIndex: 'fileFormat', width: 120, render: (v: string) => v || '—' },
    { title: t('file.size', '文件大小'), dataIndex: 'sizeBytes', width: 120, render: (v: number) => human(v) },
    { title: t('file.tags', '标签'), width: 100, render: () => <span style={{ color: '#bbb' }}>—</span> },
    { title: t('file.creator', '创建人'), dataIndex: 'createdBy', width: 120, render: (v?: string) => v || '—' },
    { title: t('file.createdAt', '创建时间'), dataIndex: 'createdAt', width: 180, render: (v: string) => v?.slice(0, 19) || '—' },
    {
      title: t('apidef.colAction', '操作'),
      width: 140,
      render: (_v, f) => (
        <Space size={4}>
          <Button type="link" size="small" onClick={() => download(f)}>{t('file.download', '下载')}</Button>
          <Button type="link" size="small" danger onClick={() => remove(f)}>{t('a.delete', '删除')}</Button>
        </Space>
      ),
    },
  ]

  if (!projectId) return <div style={{ padding: 48 }}><Empty description={t('common.selectProject', '请先在顶部选择项目')} /></div>

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* 左侧文件树(简化:全部文件 / 未规划文件) */}
      <div style={{ width: 220, flexShrink: 0, borderRight: '1px solid #f0f0f0', padding: 10, background: '#fff', overflow: 'auto' }}>
        <div style={{ fontSize: 13, color: '#8a9099', padding: '4px 8px' }}>{t('file.myFiles', '我的文件')} (0)</div>
        <div style={{ display: 'flex', alignItems: 'center', padding: '7px 10px', borderRadius: 6, background: '#f3eaff', color: '#7c3aed', fontSize: 13 }}>
          <FolderOutlined style={{ marginRight: 6 }} />{t('file.allFiles', '全部文件')} ({files.length})
        </div>
        <Input allowClear prefix={<SearchOutlined style={{ color: '#bbb' }} />} placeholder={t('file.searchName', '输入名称搜索')} style={{ margin: '8px 0' }} value={q} onChange={(e) => setQ(e.target.value)} />
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '7px 10px', fontSize: 13, color: '#5b6470' }}>
          <span>{t('file.unfiled', '未规划文件')}</span><span style={{ color: '#a8adb5' }}>{files.length}</span>
        </div>
      </div>
      {/* 右侧列表 */}
      <div style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: 12, background: '#f5f6f8' }}>
        <Card size="small" styles={{ body: { padding: 12 } }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            <Button type="primary" onClick={() => setAddOpen(true)}>{t('file.add', '添加文件')}</Button>
            <div style={{ flex: 1 }} />
            <Select size="small" value={fmt} onChange={setFmt} style={{ width: 160 }} options={[{ value: 'all', label: `${t('file.format', '文件格式')} ${t('scenario.allData', '全部')}` }, ...formats.map((f) => ({ value: f, label: f }))]} />
            <Input allowClear size="small" prefix={<SearchOutlined style={{ color: '#bbb' }} />} placeholder={t('file.searchName', '输入名称搜索')} style={{ width: 220 }} value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <Table<ProjectFile> rowKey="id" size="middle" loading={loading} dataSource={rows} columns={cols} pagination={{ pageSize: 20, size: 'small', showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}` }} locale={{ emptyText: <Empty description={t('file.empty', '暂无文件')} /> }} />
        </Card>
      </div>
      <AddFileDrawer open={addOpen} projectId={projectId} onClose={() => setAddOpen(false)} onUploaded={() => { setAddOpen(false); load() }} t={t} />
    </div>
  )
}

type TFn = (k: string, d?: string) => string

function AddFileDrawer({ open, projectId, onClose, onUploaded, t }: { open: boolean; projectId: string; onClose: () => void; onUploaded: () => void; t: TFn }) {
  const [kind, setKind] = useState('normal')
  const [file, setFile] = useState<File | null>(null)
  const [busy, setBusy] = useState(false)

  const fileToBase64 = (f: File) =>
    new Promise<string>((resolve, reject) => {
      const r = new FileReader()
      r.onload = () => resolve(String(r.result).split(',')[1] || '')
      r.onerror = reject
      r.readAsDataURL(f)
    })

  const doUpload = async () => {
    if (!file) { message.warning(t('file.selectFirst', '请先选择文件')); return }
    if (file.size > 1.4 * 1024 * 1024) { message.warning(t('file.tooBig', '当前仅支持 ≤1.4MB 的文件(受请求体上限)')); return }
    setBusy(true)
    try {
      const contentBase64 = await fileToBase64(file)
      const dot = file.name.lastIndexOf('.')
      const fileFormat = dot >= 0 ? file.name.slice(dot + 1).toLowerCase() : ''
      await api.uploadProjectFile({ projectId, name: file.name, fileFormat, sizeBytes: file.size, contentBase64 })
      message.success(t('file.uploaded', '上传成功'))
      setFile(null)
      onUploaded()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('file.uploadFailed', '上传失败'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Drawer
      open={open}
      onClose={onClose}
      width={560}
      title={t('file.add', '添加文件')}
      footer={<div style={{ textAlign: 'right' }}><Space><Button onClick={onClose}>{t('a.cancel', '取消')}</Button><Button type="primary" loading={busy} onClick={doUpload}>{t('file.startUpload', '开始上传')}</Button></Space></div>}
    >
      <div style={{ fontSize: 13, color: '#5b6470', marginBottom: 8 }}>{t('file.type', '文件类型')}</div>
      <Segmented
        block
        value={kind}
        onChange={(v) => setKind(v as string)}
        options={[
          { value: 'normal', label: <div style={{ padding: '6px 0' }}><div style={{ fontWeight: 600 }}>{t('file.normal', '常规文件')}</div><div style={{ fontSize: 12, color: '#8a9099' }}>{t('file.normalDesc', '所有文件类型')}</div></div> },
          { value: 'jar', label: <div style={{ padding: '6px 0' }}><div style={{ fontWeight: 600 }}>{t('file.jar', 'JAR 文件')}</div><div style={{ fontSize: 12, color: '#8a9099' }}>{t('file.jarDesc', '用于接口测试的文件')}</div></div> },
        ]}
      />
      <div style={{ marginTop: 16 }}>
        <Upload.Dragger
          maxCount={1}
          accept={kind === 'jar' ? '.jar' : undefined}
          beforeUpload={(f) => { setFile(f); return false }}
          onRemove={() => setFile(null)}
        >
          <p className="ant-upload-drag-icon"><InboxOutlined style={{ color: '#7c3aed' }} /></p>
          <p className="ant-upload-text">{t('file.dropHint', '拖拽或点击此区域选择文件')}</p>
          <p className="ant-upload-hint" style={{ fontSize: 12 }}>{t('file.sizeHint', '支持任意文件类型,当前演示限 ≤1.4MB')}</p>
        </Upload.Dragger>
      </div>
      {file && <div style={{ marginTop: 12, fontSize: 13 }}><Tag color="green">{t('file.selected', '已选')}</Tag>{file.name}</div>}
    </Drawer>
  )
}
