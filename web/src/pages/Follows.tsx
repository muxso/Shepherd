import { useEffect, useState } from 'react'
import { Card, Empty, Spin, Tag } from 'antd'
import { BugOutlined, StarOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { api, type Bug } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader, SelectProjectEmpty } from '../components/Page'

// 关注 = 我盯的资产。后端 /follow 是通用关注(projectId + entityType + entityId),
// 当前只有缺陷域实际写入 entityType=bug;其他资产(需求/用例/计划)接入后在此加区即可。
// 口径:GET /follow/mine?entityType=bug 拿我关注的缺陷 id,再与项目缺陷列表求交集补齐标题/状态。

const bugColor = (s: string) => {
  const v = (s || '').toUpperCase()
  if (v === 'RESOLVED' || v === 'CLOSED') return 'green'
  if (v === 'REJECTED') return 'red'
  if (v === 'NEW' || v === 'REOPENED') return 'orange'
  return 'blue'
}
const BUG_ZH: Record<string, string> = {
  NEW: '新建', RESOLVED: '已解决', CLOSED: '已关闭', REOPENED: '重新打开', REJECTED: '已拒绝',
}

export default function Follows() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const nav = useNavigate()
  const [loading, setLoading] = useState(false)
  const [followedBugs, setFollowedBugs] = useState<Bug[]>([])

  useEffect(() => {
    if (!projectId) return
    setLoading(true)
    Promise.all([
      api.myFollows(projectId, 'bug').then((r) => r.entityIds || []).catch(() => [] as string[]),
      api.bugs(projectId).catch(() => [] as Bug[]),
    ])
      .then(([ids, bugs]) => {
        const idSet = new Set(ids)
        setFollowedBugs(bugs.filter((b) => idSet.has(b.id)))
      })
      .finally(() => setLoading(false))
  }, [projectId])

  if (!projectId) return <SelectProjectEmpty />

  return (
    <PageContainer>
      <PageHeader
        title={t('home.follow.title', '关注')}
        subtitle={t('home.follow.scopeHint', '关注功能目前覆盖缺陷;其他资产的关注后续接入。')}
      />
      <PageBody>
        <Spin spinning={loading}>
          <div style={{ maxWidth: 720 }}>
            <Card
              size="small"
              title={
                <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
                  <span style={{ color: 'var(--brand)' }}><BugOutlined /></span>
                  <span>{t('home.follow.myBugs', '我关注的缺陷')}</span>
                  <span
                    style={{
                      minWidth: 20,
                      padding: '0 6px',
                      borderRadius: 10,
                      fontSize: 12,
                      lineHeight: '18px',
                      textAlign: 'center',
                      background: followedBugs.length ? 'var(--brand-soft)' : 'var(--border-soft)',
                      color: followedBugs.length ? 'var(--brand)' : 'var(--text-3)',
                    }}
                  >
                    {followedBugs.length}
                  </span>
                </span>
              }
              styles={{ body: { padding: followedBugs.length ? '4px 0' : '24px' } }}
            >
              {followedBugs.length === 0 ? (
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description={
                    <span style={{ color: 'var(--text-3)' }}>
                      {t('home.follow.empty', '暂无关注的缺陷,可在缺陷详情里点击关注。')}
                    </span>
                  }
                />
              ) : (
                followedBugs.map((b) => (
                  <div
                    key={b.id}
                    onClick={() => nav('/bug')}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: 12,
                      padding: '8px 16px',
                      cursor: 'pointer',
                      borderRadius: 6,
                      color: 'var(--text)',
                    }}
                    onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--brand-soft)' }}
                    onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent' }}
                  >
                    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, overflow: 'hidden' }}>
                      <StarOutlined style={{ color: 'var(--brand)' }} />
                      <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {b.title || b.id}
                      </span>
                    </span>
                    <Tag color={bugColor(b.status)} style={{ marginInlineEnd: 0 }}>
                      {t(`bug.st.${b.status}`, BUG_ZH[b.status] || b.status)}
                    </Tag>
                  </div>
                ))
              )}
            </Card>
          </div>
        </Spin>
      </PageBody>
    </PageContainer>
  )
}
