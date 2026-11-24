import { useState } from 'react'
import { Input } from 'antd'
import { message } from '../feedback'
import { useI18n } from '../i18n'
import { parseCurl, type CurlRequest } from '../curl'
import EditDrawer from './EditDrawer'

export const HTTP_METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']

export default function CurlImportDrawer({ open, onClose, onImport }: {
  open: boolean
  onClose: () => void
  onImport: (parsed: CurlRequest) => void
}) {
  const { t } = useI18n()
  const [curlText, setCurlText] = useState('')

  const importCurl = () => {
    const parsed = parseCurl(curlText)
    if (!parsed || !HTTP_METHODS.includes(parsed.method)) {
      message.error(t('apidef.curlUnsupported', '无法导入此 cURL。请检查命令，或粘贴包含内联请求体的 cURL (bash)。'))
      return
    }
    onImport(parsed)
    setCurlText('')
    onClose()
    message.success(t('apidef.curlImported', '已导入 cURL'))
  }

  return (
    <EditDrawer
      title={t('apidef.importCurl', '导入 cURL')}
      open={open}
      onCancel={onClose}
      onOk={importCurl}
      okText={t('a.import', '导入')}
      cancelText={t('a.cancel', '取消')}
      width={680}
    >
      <Input.TextArea
        rows={10}
        autoFocus
        aria-label={t('apidef.importCurl', '导入 cURL')}
        value={curlText}
        onChange={(e) => setCurlText(e.target.value)}
        placeholder={"curl -X POST 'https://api.example.com/login' \\\n  -H 'Content-Type: application/json' \\\n  -d '{\"user\":\"admin\"}'"}
        className="ms-mono"
      />
    </EditDrawer>
  )
}
