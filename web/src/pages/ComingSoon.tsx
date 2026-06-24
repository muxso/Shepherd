import { Empty } from 'antd'
import { useI18n } from '../i18n'

// 通用占位页(功能即将接入)。title 为页面名。
export default function ComingSoon({ title }: { title: string }) {
  const { t } = useI18n()
  return (
    <div style={{ padding: 16, height: '100%' }}>
      <div style={{ background: 'var(--panel)', borderRadius: 8, height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <Empty description={`${title} · ${t('common.comingSoon', '即将接入')}`} />
      </div>
    </div>
  )
}
