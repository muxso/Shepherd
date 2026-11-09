import { test, expect, type Page } from '@playwright/test'

async function setup(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem('shepherd.token', 'qa-test')
    localStorage.setItem('shepherd.projectId', 'project')
    sessionStorage.setItem('rag_intro_seen', '1')
  })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const url = new URL(route.request().url())
    let body: unknown = { items: [], total: 0 }
    if (url.pathname === '/auth/me') body = { userId: 'user', permissions: ['PROJECT:READ', 'RAG:READ'] }
    if (url.pathname === '/organization') body = { items: [{ id: 'org' }] }
    if (url.pathname === '/project') body = { items: [{ id: 'project', name: 'Project', organizationId: 'org' }] }
    if (url.pathname === '/rag/stats') body = { documents: 3, chunks: 6 }
    if (url.pathname === '/rag/visibility-group') body = []
    if (url.pathname === '/rag/document/document') {
      expect(url.searchParams.get('chunkId')).toBe('chunk')
      body = { documentId: 'document', projectId: 'project', title: 'Original source', sourceType: 'code', indexedAt: 1794146400000,
        reference: { occurred_at: '2026-11-08', source_links: [{ label: 'Original commit', url: 'https://example.com/commit/abc123' }] },
        chunks: [{ id: 'chunk', heading: 'Source file', content: 'Source body evidence', index: 0 }], truncated: false }
    }
    if (url.pathname === '/rag/ask/stream') {
      const request = route.request().postDataJSON()
      expect(request.projectId).toBe('project')
      const events = [
        ['sources', { sources: [{ title: 'Team activity source', url: 'https://example.com/activity', occurred_at: '2026-11-08' }] }],
        ['chart', { kind: 'activity_heatmap', title: '各组活动热力图', metric: '已采集代码提交', from: '2026-11-02', through: '2026-11-08', dates: ['2026-11-02', '2026-11-03'], rows: [{ label: '研发组', group: '研发组', values: [2, 3] }, { label: '产品组', group: '产品组', values: [0, 0] }], coverageComplete: false }],
        ['chunk', { delta: '查询范围：2026-11-02 ～ 2026-11-08，已采集 5 次提交。[1]' }],
        ['done', { grounded: true }],
      ]
      return route.fulfill({ contentType: 'text/event-stream', body: events.map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('') })
    }
    await route.fulfill({ json: body })
  })
}

test('statistics replies render charts and clickable dated sources without a model explanation', async ({ page }, testInfo) => {
  await setup(page)
  await page.goto('/p/project/chat')
  await page.locator('textarea:not([aria-hidden="true"])').fill('云效数据按组发我，每个组的代码热力图')
  await page.locator('textarea:not([aria-hidden="true"])').press('Enter')
  await expect(page.getByRole('region', { name: '各组活动热力图' })).toBeVisible()
  await expect(page.getByLabel('研发组 2026-11-02 2', { exact: true })).toBeVisible()
  await expect(page.getByLabel('产品组 2026-11-02 0', { exact: true })).toBeVisible()
  const source = page.getByRole('link', { name: /Team activity source/ })
  await expect(source).toHaveAttribute('href', 'https://example.com/activity')
  await expect(source).toContainText('2026-11-08')
  await expect(page.getByText('未在知识库中找到相关内容')).toHaveCount(0)
  await page.screenshot({ path: testInfo.outputPath('grounded-activity-heatmap.png') })
})

test('an internal source link opens an authorized original document and its external commit', async ({ page }) => {
  await setup(page)
  await page.goto('/p/project/chat?document=document&chunk=chunk')
  const drawer = page.getByRole('dialog')
  await expect(drawer.getByText('Source body evidence', { exact: false })).toBeVisible()
  await expect(drawer.getByRole('link', { name: /Original commit/ })).toHaveAttribute('href', 'https://example.com/commit/abc123')
  await expect(drawer.getByText(/2026-11-08/).first()).toBeVisible()
})

test('onboarding maintenance publishes explicit sources and visibility without fictional policies', async ({ page }) => {
  await setup(page)
  let published: Record<string, unknown> | undefined
  await page.route('**/rag/onboarding*', async route => {
    if (route.request().method() === 'POST') {
      published = route.request().postDataJSON()
      return route.fulfill({ json: { documentId: 'guide', version: 1, embedded: true } })
    }
    return route.fulfill({ json: { items: [] } })
  })
  await page.goto('/p/project/chat')
  await page.getByRole('button', { name: '入职知识', exact: true }).click()
  await expect(page.getByText('未维护的制度不会自动生成。', { exact: false })).toBeVisible()
  await page.getByRole('button', { name: '维护指南／FAQ' }).click()
  const modal = page.getByRole('dialog').last()
  await modal.getByLabel('资料标题').fill('Access guide')
  await modal.getByLabel('原始制度／文档链接').fill('https://example.com/access')
  await modal.getByLabel('指南内容／常见问答（Markdown）').fill('# Access guide\nAsk the project owner for access.')
  await modal.getByRole('button', { name: '发布并更新问答知识' }).click()
  await expect.poll(() => published?.sourceUrl).toBe('https://example.com/access')
  expect(published?.projectId).toBe('project')
  expect(published?.visibilityGroups).toEqual([])
  expect(published?.expectedVersion).toBe(0)
})

test('channel admin starts disabled and requires enabling backend group binding', async ({ page }, testInfo) => {
  await setup(page)
  await page.route('**/auth/me', route => route.fulfill({ json: { userId: 'user', permissions: ['SYSTEM_USER:UPDATE', 'SYSTEM_USER:READ', 'PROJECT:READ', 'RAG:READ'] } }))
  await page.route('**/system/rag/config', route => route.fulfill({ json: { embedUrl: '', embedModel: '', embedDim: 1024, chatUrl: '', chatModel: '', maxTokens: 2000, topK: 8, rerank: true } }))
  await page.route('**/system/rag/channels', route => route.fulfill({ json: { items: [{ id: 'connector', name: 'Project bot', serviceUserId: 'bot', enabled: true, allowBinding: false, conversations: [], pending: [], status: { connected: true, activeQueries: 0, reportedAt: '2026-11-08T14:00:00Z' } }] } }))
  await page.route('**/system/user*', route => route.fulfill({ json: { items: [{ id: 'bot', name: 'Service account' }] } }))
  await page.goto('/system/params')
  await expect(page.getByText('企业微信 · 会话授权与连接')).toBeVisible()
  await expect(page.getByRole('button', { name: '生成绑定码', exact: true })).toBeDisabled()
  await page.getByRole('button', { name: '添加机器人渠道' }).click()
  const modal = page.getByRole('dialog')
  await expect(modal.getByLabel('开启渠道授权')).not.toBeChecked()
  await expect(modal.getByLabel('允许一次性绑定群')).not.toBeChecked()
  await page.screenshot({ path: testInfo.outputPath('rag-channel-permissions.png') })
})


test('pending direct authorization reuses the channel account and preserves group access', async ({ page }) => {
  await setup(page)
  await page.route('**/auth/me', route => route.fulfill({ json: { userId: 'user', permissions: ['SYSTEM_USER:UPDATE', 'SYSTEM_USER:READ', 'PROJECT:READ', 'RAG:READ'] } }))
  await page.route('**/system/rag/config', route => route.fulfill({ json: { embedUrl: '', embedModel: '', embedDim: 1024, chatUrl: '', chatModel: '', maxTokens: 2000, topK: 8, rerank: true } }))
  const group = { kind: 'group', externalId: 'group-a', projectId: 'project', enabled: true }
  const channel = { id: 'connector', name: 'Project bot', serviceUserId: 'bot', enabled: true, allowBinding: false, conversations: [group], pending: [{ kind: 'direct', externalId: 'user-a', senderId: 'user-a', seenAt: '2026-11-09T00:00:00Z' }] }
  await page.route('**/system/rag/channels', route => route.fulfill({ json: { items: [channel] } }))
  let saved: Record<string, unknown> | undefined
  await page.route('**/system/rag/channels/connector', route => { saved = route.request().postDataJSON(); return route.fulfill({ json: { id: 'connector' } }) })
  await page.goto('/system/params')
  await page.getByRole('button', { name: '展开行' }).click()
  await page.getByRole('button', { name: '选择项目并授权' }).click()
  const modal = page.getByRole('dialog')
  await expect(modal).toContainText('user-a')
  await expect(modal).toContainText('Project bot')
  await expect(modal.getByRole('button', { name: '授权此会话' })).toBeDisabled()
  await modal.getByRole('combobox', { name: '会话授权项目' }).click()
  await page.locator('.ant-select-dropdown').getByTitle('Project', { exact: true }).click()
  await modal.getByRole('button', { name: '授权此会话' }).click()
  await expect.poll(() => saved?.serviceUserId).toBe('bot')
  expect(saved?.allowBinding).toBe(false)
  expect(saved?.conversations).toEqual([group, { kind: 'direct', externalId: 'user-a', projectId: 'project', enabled: true }])
})


test('answer heatmaps expand daily code indicators and hide zero bugs by default', async ({ page }, testInfo) => {
  await setup(page)
  await page.route('**/rag/ask/stream', route => {
    const empty = { count: 0, lastTime: null, additions: null, deletions: null, requirements: 0, bugs: 0, created: 0, published: 0, testcases: 0 }
    const chart = { kind: 'activity_heatmap', title: '成员展开热力图', metric: '已采集代码提交', detailMode: 'code', expanded: true, from: '2026-11-07', through: '2026-11-08', dates: ['2026-11-07', '2026-11-08'], rows: [{ label: '成员甲', group: '研发组', values: [2, 0], cells: [{ ...empty, count: 2, lastTime: '20:35', additions: 6191, deletions: 2150, requirements: 3 }, { ...empty, bugs: 2 }], totals: { ...empty, count: 2, requirements: 3, bugs: 2 } }] }
    return route.fulfill({ contentType: 'text/event-stream', body: [['chart', chart], ['chunk', { delta: '已核对提交记录。' }], ['done', { grounded: true }]].map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('') })
  })
  await page.goto('/p/project/chat')
  await page.locator('textarea:not([aria-hidden="true"])').fill('团队代码热力图')
  await page.locator('textarea:not([aria-hidden="true"])').press('Enter')
  const chart = page.getByRole('region', { name: '成员展开热力图' })
  await expect(chart.getByText('20:35', { exact: true })).toBeVisible()
  await expect(chart.getByText('+6,191 / −2,150')).toBeVisible()
  await expect(chart.getByText('关联需求 3')).toBeVisible()
  await expect(chart.getByText('Bug 2', { exact: true })).toBeVisible()
  await expect(chart.getByText('Bug 0', { exact: true })).toHaveCount(0)
  await expect(chart.getByText('周六', { exact: true })).toBeVisible()
  await page.screenshot({ path: testInfo.outputPath('rag-expanded-heatmap.png') })
  await chart.getByRole('button', { name: '收起格子指标' }).click()
  await expect(chart.getByText('20:35', { exact: true })).toHaveCount(0)
  await expect(chart.getByRole('button', { name: '展开格子指标' })).toBeVisible()
})
