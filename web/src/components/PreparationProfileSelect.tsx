import { useEffect, useState } from 'react'
import { Select, Space, Typography } from 'antd'
import { api, type Environment } from '../api'
import { environmentPluginApi, type Profile } from '../plugins/environment/api'

export default function PreparationProfileSelect({ projectId, environmentId, value, onChange }: {
  projectId?: string
  environmentId?: string
  value: string
  onChange: (value: string) => void
}) {
  const [envs, setEnvs] = useState<Environment[]>([])
  const [localEnv, setLocalEnv] = useState('')
  const [profiles, setProfiles] = useState<Profile[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const environment = environmentId || localEnv

  useEffect(() => {
    if (environmentId || !projectId) return
    let active = true
    api.environments(projectId).then(items => {
      if (active) setEnvs(items.filter(item => item.enabled !== false))
    }).catch(() => { if (active) setError('无法加载执行环境') })
    return () => { active = false }
  }, [environmentId, projectId])

  useEffect(() => {
    setProfiles([])
    if (!environment) {
      setLoading(false)
      return
    }
    let active = true
    setError('')
    setLoading(true)
    Promise.all([environmentPluginApi.configuration(environment), environmentPluginApi.capabilities()])
      .then(([config, capabilities]) => {
        if (!active) return
        if (!config.permissions.execute) {
          setError('当前账号没有此环境的插件执行权限')
          return
        }
        const providers = new Set(capabilities.items.filter(item => item.capabilities.some(cap => cap.toLowerCase() === 'preprocess')).map(item => item.id))
        setProfiles(config.profiles.filter(profile => profile.enabled && providers.has(profile.pluginId)))
      })
      .catch(() => { if (active) setError('无法加载前置插件配置') })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [environment])

  const options = profiles.map(profile => ({ value: profile.id, label: profile.name }))
  if (value && !profiles.some(profile => profile.id === value)) {
    options.push({ value, label: loading || !environment ? '已引用的配置' : '已引用的配置（当前环境不可用）' })
  }

  return <Space direction="vertical" style={{ width: '100%' }}>
    {!environmentId && <Select showSearch optionFilterProp="label" aria-label="配置来源环境"
      placeholder="选择环境以查看插件配置" value={localEnv || undefined} onChange={setLocalEnv}
      options={envs.map(env => ({ value: env.id, label: env.name }))} style={{ width: '100%' }} />}
    <Select showSearch optionFilterProp="label" aria-label="前置插件配置" placeholder="搜索并选择前置插件配置"
      disabled={!environment} loading={loading} value={value || undefined} onChange={onChange} options={options}
      notFoundContent="此环境没有可用的前置插件配置" style={{ width: '100%' }} />
    {error && <Typography.Text type="danger">{error}</Typography.Text>}
  </Space>
}
