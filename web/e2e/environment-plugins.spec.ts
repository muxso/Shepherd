import { test, expect, type Page } from '@playwright/test'
import type { Binding, Configuration, Credential, Descriptor, NewProfile, Profile } from '../src/plugins/environment/api'

const postgres: Descriptor = {
  id: 'postgres', name: 'PostgreSQL', section: 'database', version: '1.0.0', apiVersion: 1,
  capabilities: ['execute', 'connection_test'], operations: ['test', 'execute'],
  configSchema: { version: 1, fields: [
    { name: 'host', label: 'Host', required: true, kind: { type: 'string', max_length: 253 } },
    { name: 'port', label: 'Port', required: false, kind: { type: 'integer', min: 1, max: 65535 } },
    { name: 'database', label: 'Database', required: true, kind: { type: 'string', max_length: 128 } },
    { name: 'user', label: 'User', required: true, kind: { type: 'string', max_length: 128 } },
    { name: 'password', label: 'Password', required: false, kind: { type: 'secret' } },
  ] },
}

const redis: Descriptor = {
  id: 'redis', name: 'Redis', section: 'redis', version: '1.0.0', apiVersion: 1,
  capabilities: ['execute', 'connection_test'], operations: ['test', 'execute'],
  configSchema: { version: 1, fields: [
    { name: 'host', label: 'Host', required: true, kind: { type: 'string', max_length: 253 } },
    { name: 'port', label: 'Port', required: false, kind: { type: 'integer', min: 1, max: 65535 } },
    { name: 'database', label: 'Database', required: false, kind: { type: 'integer', min: 0, max: 2147483647 } },
    { name: 'username', label: 'Username', required: false, kind: { type: 'string', max_length: 128 } },
    { name: 'password', label: 'Password', required: false, kind: { type: 'secret' } },
    { name: 'tls', label: 'TLS', required: false, kind: { type: 'choice', options: ['require', 'disable'] } },
  ] },
}

const fixturePrivateKey = '-----BEGIN OPENSSH PRIVATE KEY-----\nfixture-key-data\n-----END OPENSSH PRIVATE KEY-----'
const ssh: Descriptor = {
  id: 'ssh', name: 'SSH', section: 'ssh', version: '1.0.0', apiVersion: 1,
  capabilities: ['execute', 'connection_test'], operations: ['test', 'execute'],
  configSchema: { version: 1, fields: [
    { name: 'host', label: 'Host', required: true, kind: { type: 'string', max_length: 253 } },
    { name: 'username', label: 'Username', required: true, kind: { type: 'string', max_length: 128 } },
    { name: 'host_key_sha256', label: 'Host key SHA256', required: true, kind: { type: 'string', max_length: 128 } },
    { name: 'private_key', label: 'Private key', required: false, kind: { type: 'secret_multiline' } },
  ] },
}

async function fixture(page: Page, enabled = true, readonly = false) {
  const profiles: Profile[] = readonly ? [{ id: 'profile-1', projectId: 'project', pluginId: 'postgres', name: 'Existing database', enabled: true, revision: 1, config: { schemaVersion: 1, values: { host: 'localhost', database: 'fixture', user: 'reader' }, secrets: {} } }] : []
  const bindings: Binding[] = []
  const credentials: Credential[] = []
  const saved: NewProfile[] = []
  const executed: unknown[] = []
  let tested = 0
  let configurationReads = 0
  await page.addInitScript(() => { localStorage.setItem('shepherd.token', 'environment-plugin-fixture'); localStorage.setItem('shepherd.projectId', 'project') })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const path = new URL(route.request().url()).pathname
    const method = route.request().method()
    let body: unknown = { items: [], total: 0 }
    if (path === '/organization') body = { items: [{ id: 'organization', name: 'Organization' }], totalPages: 1 }
    else if (path === '/project') body = { items: [{ id: 'project', name: 'Project', organizationId: 'organization' }], totalPages: 1 }
    else if (path === '/api/environment') body = [{ id: 'environment', projectId: 'project', name: 'Test environment', baseUrl: '', headers: [], variables: {}, enabled: true }]
    else if (path === '/api/plugins') body = { items: [] }
    else if (path === '/api/environment-plugins/capabilities') body = { enabled, items: enabled ? [postgres, redis, ssh] : [] }
    else if (path === '/api/environment-plugins/environments/environment') {
      configurationReads++
      body = { profiles, bindings, credentials, permissions: { manage: !readonly, execute: !readonly } } satisfies Configuration
    } else if (path.endsWith('/credentials') && method === 'POST') {
      const input = route.request().postDataJSON() as { label: string; value: string }
      expect(input.value).toBe(input.label.endsWith('Private key') ? fixturePrivateKey : 'private-fixture-password')
      const id = `credential-${credentials.length + 1}`
      credentials.push({ id, label: input.label, environmentId: 'environment' }); body = { reference: id }
    } else if (path.endsWith('/profiles') && method === 'POST') {
      const input = route.request().postDataJSON() as NewProfile
      saved.push(input)
      const record: Profile = { ...input, id: `profile-${profiles.length + 1}`, projectId: 'project', revision: 1 }
      profiles.push(record); body = record
    } else if (path.includes('/bindings/') && method === 'PUT') {
      const id = path.split('/').at(-1)!
      const input = route.request().postDataJSON() as Omit<Binding, 'profileId'>
      bindings.push({ ...input, profileId: id }); body = { bound: true }
    } else if (path.endsWith('/execute')) {
      const input = route.request().postDataJSON() as { command?: string; extract?: { variable: string }[] }
      executed.push(input)
      const failed = input.extract?.some(e => e.variable === 'missing') ?? false
      const plugin = profiles.find(profile => profile.id === path.split('/').at(-2))?.pluginId
      if (plugin === 'ssh') body = { status: 'failure', data: { operation: 'executed', hostKeySha256: `SHA256:${'A'.repeat(43)}`, stdout: { encoding: 'utf8', value: 'output\n' }, stderr: { encoding: 'utf8', value: 'failure\n' }, termination: { kind: 'exit', code: 7 }, elapsedMs: 8 }, variables: {}, assertions: [{ id: 'ssh.exit', passed: false, message: 'command exited with status 7' }] }
      else if (input.command) body = { status: failed ? 'failure' : 'success', data: { kind: 'array', value: ['hello world', null], elapsedMs: 4 }, variables: failed ? {} : { first: 'hello world', nil: 'null' }, assertions: failed ? [{ id: 'extract.missing', passed: false, message: 'required Redis result path is missing' }] : [] }
      else body = { status: failed ? 'failure' : 'success', data: { columns: [{ name: 'id', sqlType: 'INT8' }, { name: 'note', sqlType: 'TEXT' }], rows: [['9007199254740993', null]], rowsAffected: 0, truncated: !failed, elapsedMs: 12 }, variables: failed ? {} : { id: '9007199254740993' }, assertions: failed ? [{ id: 'extract.missing', passed: false, message: 'required result cell is missing' }] : [] }
    } else if (path.endsWith('/test')) {
      tested++; body = { status: 'success', data: { output: 'rows_affected=1' }, variables: {}, assertions: [] }
    }
    await route.fulfill({ json: body })
  })
  return { saved, profiles, bindings, executed, tested: () => tested, reads: () => configurationReads }
}
async function open(page: Page) {
  await page.goto('/p/project/environment')
  await page.getByText('Test environment', { exact: true }).click()
  await page.getByRole('tab', { name: '数据库', exact: true }).click()
}

test('plugin configuration saves secret references, binds, tests, and survives reload without revealing credentials', async ({ page }) => {
  const data = await fixture(page)
  await open(page)
  await page.getByRole('button', { name: '新增连接配置' }).click()
  const dialog = page.getByRole('dialog')
  await dialog.getByLabel('配置名称', { exact: true }).fill('Local PostgreSQL')
  await dialog.getByLabel('Host', { exact: true }).fill('127.0.0.1')
  await dialog.getByLabel('Port', { exact: true }).fill('5432')
  await dialog.getByLabel('Database', { exact: true }).fill('fixture')
  await dialog.getByLabel('User', { exact: true }).fill('tester')
  await dialog.getByLabel('Password', { exact: true }).fill('private-fixture-password')
  await dialog.getByRole('button', { name: '保存并测试' }).click()
  await expect(dialog.getByText('连接测试通过', { exact: true })).toBeVisible()
  expect(data.saved).toHaveLength(1)
  expect(data.saved[0].config.secrets.password).toBe('credential-1')
  expect(JSON.stringify(data.saved[0])).not.toContain('private-fixture-password')
  expect(data.bindings[0].profileId).toBe(data.profiles[0].id)
  expect(data.tested()).toBe(1)
  await dialog.getByRole('button', { name: '关闭', exact: true }).click()
  await expect(dialog).not.toBeVisible()
  await page.reload()
  await page.getByText('Test environment', { exact: true }).click()
  await page.getByRole('tab', { name: '数据库', exact: true }).click()
  await expect(page.getByText('Local PostgreSQL', { exact: true })).toBeVisible()
  await page.getByRole('button', { name: '编辑', exact: true }).click()
  await expect(dialog.getByLabel('Host', { exact: true })).toHaveValue('127.0.0.1')
  await expect(dialog.getByLabel('Password', { exact: true })).toHaveValue('')
  await dialog.getByRole('button', { name: '关闭', exact: true }).click()
  await expect(dialog).not.toBeVisible()
  await page.getByRole('tab', { name: 'HOST', exact: true }).click()
  await expect(page.getByText('此项能力尚未安装可用插件', { exact: true })).toBeVisible()
  expect(data.tested()).toBe(1)
})

test('disabled plugin shows its state and never requests provider configuration', async ({ page }) => {
  const data = await fixture(page, false)
  await open(page)
  await expect(page.getByText('环境配置插件未启用', { exact: true })).toBeVisible()
  await expect(page.getByRole('button', { name: '新增连接配置' })).toHaveCount(0)
  expect(data.reads()).toBe(0)
})

test('project viewers can inspect profiles but cannot edit or execute them', async ({ page }) => {
  await fixture(page, true, true)
  await open(page)
  await expect(page.getByRole('button', { name: '新增连接配置' })).toBeDisabled()
  await expect(page.getByRole('button', { name: '测试连接', exact: true })).toBeDisabled()
  await page.getByRole('button', { name: '编辑', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByLabel('Host', { exact: true })).toBeDisabled()
  await expect(dialog.getByRole('button', { name: '保存配置', exact: true })).toBeDisabled()
})

test('database query uses the saved binding and renders rows, extraction failures, truncation, and report download', async ({ page }) => {
  const data = await fixture(page)
  data.profiles.push({ id: 'profile-query', projectId: 'project', pluginId: 'postgres', name: 'Query database', enabled: true, revision: 1, config: { schemaVersion: 1, values: { host: 'localhost', database: 'fixture', user: 'reader' }, secrets: {} } })
  data.bindings.push({ profileId: 'profile-query', slot: 'connection', priority: 0, enabled: true })
  await open(page)
  await page.getByRole('button', { name: '数据库查询', exact: true }).click()
  const dialog = page.getByRole('dialog')
  await dialog.getByLabel('SQL', { exact: true }).fill('SELECT $1::bigint AS id')
  await dialog.getByLabel('查询参数', { exact: true }).fill('[{"type":"integer","value":42}]')
  await dialog.getByLabel('结果提取规则', { exact: true }).fill('[{"variable":"id","row":0,"column":"id"}]')
  await dialog.getByRole('button', { name: '运行查询', exact: true }).click()
  await expect(dialog.getByText('数据库执行通过', { exact: true })).toBeVisible()
  await expect(dialog.getByRole('cell', { name: '9007199254740993', exact: true })).toBeVisible()
  await expect(dialog.getByRole('cell', { name: 'NULL', exact: true })).toBeVisible()
  await expect(dialog.getByText('结果已达到行数上限，仅展示部分结果', { exact: true })).toBeVisible()
  expect(data.executed[0]).toEqual({ query: 'SELECT $1::bigint AS id', mode: 'query', parameters: [{ type: 'integer', value: 42 }], extract: [{ variable: 'id', row: 0, column: 'id' }], maxRows: 100 })
  const downloading = page.waitForEvent('download')
  await dialog.getByRole('button', { name: '导出报告', exact: true }).click()
  const download = await downloading
  expect(download.suggestedFilename()).toBe('database-report-profile-query.json')
  await dialog.getByLabel('结果提取规则', { exact: true }).fill('[{"variable":"missing","row":9,"column":0}]')
  await dialog.getByRole('button', { name: '运行查询', exact: true }).click()
  await expect(dialog.getByText('结果提取未通过', { exact: true })).toBeVisible()
  await expect(dialog.getByText('extract.missing: required result cell is missing', { exact: true })).toBeVisible()
})

test('Redis profile saves credential references and executes structured arguments with extraction and report download', async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1080 })
  const data = await fixture(page)
  await open(page)
  await page.getByRole('tab', { name: 'Redis 配置', exact: true }).click()
  await page.getByRole('button', { name: '新增连接配置', exact: true }).click()
  let dialog = page.getByRole('dialog')
  await dialog.getByLabel('配置名称', { exact: true }).fill('Redis fixture')
  await dialog.getByLabel('Host', { exact: true }).fill('127.0.0.1')
  await dialog.getByLabel('Database', { exact: true }).fill('13')
  await dialog.getByLabel('Username', { exact: true }).fill('default')
  await dialog.getByLabel('Password', { exact: true }).fill('private-fixture-password')
  await dialog.getByRole('button', { name: '保存并测试', exact: true }).click()
  await expect(dialog.getByText('连接测试通过', { exact: true })).toBeVisible()
  expect(data.saved[0].pluginId).toBe('redis')
  expect(data.saved[0].config.values.database).toBe(13)
  expect(data.saved[0].config.secrets.password).toBe('credential-1')
  await dialog.getByRole('button', { name: '关闭', exact: true }).click()
  await expect(dialog).not.toBeVisible()
  await page.getByRole('button', { name: 'Redis 命令', exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByLabel('Redis 命令名称', { exact: true }).fill('MGET')
  await dialog.getByLabel('Redis 命令参数', { exact: true }).fill('["key with spaces", "missing"]')
  await dialog.getByLabel('Redis 提取规则', { exact: true }).fill('[{"variable":"first","pointer":"/0"},{"variable":"nil","pointer":"/1"}]')
  await dialog.getByRole('button', { name: '执行命令', exact: true }).click()
  await expect(dialog.getByText('Redis 执行通过', { exact: true })).toBeVisible()
  expect(data.executed[0]).toEqual({ command: 'MGET', arguments: ['key with spaces', 'missing'], extract: [{ variable: 'first', pointer: '/0' }, { variable: 'nil', pointer: '/1' }] })
  await expect(dialog.locator('pre').first()).toContainText('hello world')
  await expect(dialog.locator('pre').last()).toContainText('"nil": "null"')
  const downloading = page.waitForEvent('download')
  await dialog.getByRole('button', { name: '导出报告', exact: true }).click()
  expect((await downloading).suggestedFilename()).toBe('redis-report-profile-1.json')
  await dialog.getByLabel('Redis 提取规则', { exact: true }).fill('[{"variable":"missing","pointer":"/5"}]')
  await dialog.getByRole('button', { name: '执行命令', exact: true }).click()
  await expect(dialog.getByText('Redis 结果提取未通过', { exact: true })).toBeVisible()
})

test('SSH profiles keep multiline keys in credential references and report separate streams and nonzero exits', async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1080 })
  const data = await fixture(page)
  await open(page)
  await page.getByRole('tab', { name: 'SSH配置', exact: true }).click()
  await page.getByRole('button', { name: '新增连接配置', exact: true }).click()
  let dialog = page.getByRole('dialog')
  await dialog.getByLabel('配置名称', { exact: true }).fill('SSH fixture')
  await dialog.getByLabel('Host', { exact: true }).fill('127.0.0.1')
  await dialog.getByLabel('Username', { exact: true }).fill('fixture')
  await dialog.getByLabel('Host key SHA256', { exact: true }).fill(`SHA256:${'A'.repeat(43)}`)
  await dialog.getByLabel('Private key', { exact: true }).fill(fixturePrivateKey)
  await dialog.getByRole('button', { name: '保存并测试', exact: true }).click()
  await expect(dialog.getByText('连接测试通过', { exact: true })).toBeVisible()
  expect(data.saved[0].config.secrets.private_key).toBe('credential-1')
  expect(data.saved[0].config.values).not.toHaveProperty('private_key')
  await expect(dialog.getByLabel('Private key', { exact: true })).toHaveValue('')
  await dialog.getByRole('button', { name: '关闭', exact: true }).click()
  await expect(dialog).not.toBeVisible()
  await page.getByRole('button', { name: 'SSH 命令', exact: true }).click()
  dialog = page.getByRole('dialog')
  const command = 'printf "output"; printf "failure" >&2; exit 7'
  await dialog.getByLabel('SSH 远端命令', { exact: true }).fill(command)
  await dialog.getByRole('button', { name: '执行 SSH 命令', exact: true }).click()
  await expect(dialog.getByText('SSH 命令未成功完成', { exact: true })).toBeVisible()
  await expect(dialog.getByText('退出码 7', { exact: true })).toBeVisible()
  await expect(dialog.locator('pre').first()).toContainText('output')
  await expect(dialog.locator('pre').last()).toContainText('failure')
  expect(data.executed[0]).toEqual({ command })
})
