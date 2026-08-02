import { useEffect, useState } from 'react'
import { Button, Card, Col, Form, Input, InputNumber, Row, Switch, Spin, Alert, Tag } from 'antd'
import { SaveOutlined, ExperimentOutlined, DatabaseOutlined, RobotOutlined } from '@ant-design/icons'
import { api, ApiError, type RagConfigBody, type RagConfigView } from '../api'
import { message } from '../feedback'
import { useI18n } from '../i18n'

// System-level RAG config, laid out in three columns matching the pipeline:
//   left = embedding (vectorize) · middle = vector store / retrieval · right = generation (output).
// Overrides the SHEPHERD_RAG_* env fallbacks and hot-reloads on the server (no restart).
// API keys are write-only: the form shows whether one is set, and only sends a key when changed.

export default function RagSettings() {
  const { t } = useI18n()
  const [form] = Form.useForm<RagConfigBody>()
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [embedKeySet, setEmbedKeySet] = useState(false)
  const [chatKeySet, setChatKeySet] = useState(false)

  useEffect(() => {
    api
      .ragConfig()
      .then((c: RagConfigView) => {
        setEmbedKeySet(c.embedKeySet)
        setChatKeySet(c.chatKeySet)
        form.setFieldsValue({
          embedUrl: c.embedUrl,
          embedModel: c.embedModel,
          embedDim: c.embedDim,
          embedKey: '',
          chatUrl: c.chatUrl,
          chatModel: c.chatModel,
          chatKey: '',
          maxTokens: c.maxTokens,
          topK: c.topK,
          rerank: c.rerank,
        })
      })
      .catch((e) => message.error(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }, [form])

  const onSave = async () => {
    const v = await form.validateFields()
    setSaving(true)
    try {
      // Empty key field = keep the stored key; only send a non-empty value.
      const body: RagConfigBody = { ...v }
      if (!body.embedKey) delete body.embedKey
      if (!body.chatKey) delete body.chatKey
      await api.saveRagConfig(body)
      message.success(t('rag.saved', '已保存,已即时生效'))
      if (body.embedKey) setEmbedKeySet(true)
      if (body.chatKey) setChatKeySet(true)
      form.setFieldsValue({ embedKey: '', chatKey: '' })
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  if (loading) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 120 }}>
        <Spin size="large" />
      </div>
    )
  }

  const required = [{ required: true, message: t('rag.required', '必填') }]

  return (
    <div style={{ maxWidth: 1240, margin: '0 auto', padding: '24px 16px' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <h2 style={{ margin: 0, fontSize: 18 }}>
          <ExperimentOutlined style={{ marginRight: 8 }} />
          {t('sys.rag', 'RAG 配置')}
        </h2>
        <Button type="primary" icon={<SaveOutlined />} loading={saving} onClick={onSave}>
          {t('common.save', '保存')}
        </Button>
      </div>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 20 }}
        message={t(
          'rag.hint',
          '用于知识问答(聊天)与需求 AI 评审的检索/生成模型。留空 API Key 表示沿用已保存的密钥,保存后即时生效。',
        )}
      />
      <Form form={form} layout="vertical" requiredMark={false}>
        <Row gutter={16}>
          {/* Left: embedding — turns text into vectors. */}
          <Col xs={24} lg={8}>
            <Card
              title={
                <span>
                  <ExperimentOutlined style={{ marginRight: 8 }} />
                  {t('rag.embedSection', '向量嵌入 Embedding')}
                </span>
              }
              styles={{ body: { paddingBottom: 4 } }}
            >
              <Form.Item name="embedUrl" label={t('rag.embedUrl', '嵌入接口地址')} rules={required}>
                <Input placeholder="https://api.openai.com/v1/embeddings" />
              </Form.Item>
              <Form.Item name="embedModel" label={t('rag.embedModel', '嵌入模型')} rules={required}>
                <Input placeholder="text-embedding-3-small" />
              </Form.Item>
              <Form.Item
                name="embedDim"
                label={t('rag.embedDim', '向量维度')}
                tooltip={t('rag.embedDimTip', '需与嵌入模型输出维度一致,修改后需重新入库')}
                rules={required}
              >
                <InputNumber min={1} max={8192} style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item
                name="embedKey"
                label={t('rag.embedKey', '嵌入 API Key')}
                extra={embedKeySet ? t('rag.keySet', '已设置(留空保持不变)') : t('rag.keyUnset', '尚未设置')}
              >
                <Input.Password placeholder={embedKeySet ? '••••••••' : 'sk-...'} autoComplete="new-password" />
              </Form.Item>
            </Card>
          </Col>

          {/* Middle: vector store — where vectors live and how many are recalled. */}
          <Col xs={24} lg={8}>
            <Card
              title={
                <span>
                  <DatabaseOutlined style={{ marginRight: 8 }} />
                  {t('rag.storeSection', '向量数据库')}
                </span>
              }
              styles={{ body: { paddingBottom: 4 } }}
            >
              <Form.Item label={t('rag.storeBackend', '存储后端')}>
                <Tag color="blue">PostgreSQL</Tag>
                <span style={{ color: 'var(--muted, #999)', fontSize: 12 }}>
                  {t('rag.storeBackendNote', '内置 rag_cosine 余弦检索,无需额外扩展')}
                </span>
              </Form.Item>
              <Form.Item
                name="topK"
                label={t('rag.topK', '召回条数 Top-K')}
                tooltip={t('rag.topKTip', '每次问答从库中召回的知识块数量')}
                rules={required}
              >
                <InputNumber min={1} max={20} style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item
                name="rerank"
                label={t('rag.rerank', 'LLM 重排')}
                tooltip={t('rag.rerankTip', '召回后用生成模型对相关性重新排序,更准但更慢')}
                valuePropName="checked"
              >
                <Switch />
              </Form.Item>
            </Card>
          </Col>

          {/* Right: generation — combines context into an answer. */}
          <Col xs={24} lg={8}>
            <Card
              title={
                <span>
                  <RobotOutlined style={{ marginRight: 8 }} />
                  {t('rag.chatSection', '组合输出 Generation')}
                </span>
              }
              styles={{ body: { paddingBottom: 4 } }}
            >
              <Form.Item name="chatUrl" label={t('rag.chatUrl', '生成接口地址')} rules={required}>
                <Input placeholder="https://api.openai.com/v1/chat/completions" />
              </Form.Item>
              <Form.Item name="chatModel" label={t('rag.chatModel', '生成模型')} rules={required}>
                <Input placeholder="gpt-4o-mini" />
              </Form.Item>
              <Form.Item name="maxTokens" label={t('rag.maxTokens', '生成最大 Token')} rules={required}>
                <InputNumber min={1} max={32768} style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item
                name="chatKey"
                label={t('rag.chatKey', '生成 API Key')}
                extra={chatKeySet ? t('rag.keySet', '已设置(留空保持不变)') : t('rag.keyUnset', '尚未设置')}
              >
                <Input.Password placeholder={chatKeySet ? '••••••••' : 'sk-...'} autoComplete="new-password" />
              </Form.Item>
            </Card>
          </Col>
        </Row>
      </Form>
    </div>
  )
}
