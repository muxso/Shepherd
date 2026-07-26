import { useEffect, useState } from 'react'
import { Input } from 'antd'
import { LockOutlined, UserOutlined } from '@ant-design/icons'
import { message } from '../feedback'
import { api, ApiError, userIdStore, userStore } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import AnimatedLogo from '../components/AnimatedLogo'
import './Login.css'

// Provider segment of /auth/oidc/{provider}/authorize; overridable at build time.
const OIDC_PROVIDER = (import.meta.env.VITE_OIDC_PROVIDER as string | undefined) || 'oidc'

export default function Login() {
  const { login } = useApp()
  const { t } = useI18n()
  const [mode, setModeState] = useState<'admin' | 'sso'>('admin')
  // First mount plays the default entrance; after any switch the replay
  // mirrors to follow the travel direction.
  const [switched, setSwitched] = useState(false)
  const setMode = (m: 'admin' | 'sso') => {
    setSwitched(true)
    setModeState(m)
  }
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('')
  const [loading, setLoading] = useState(false)
  const [ssoReady, setSsoReady] = useState(false)

  // SSO availability probe: authorize answers 302 when the provider is
  // configured, 404 otherwise. redirect:manual keeps the browser in place.
  useEffect(() => {
    fetch(`/auth/oidc/${OIDC_PROVIDER}/authorize`, { redirect: 'manual' })
      .then((r) => setSsoReady(r.type === 'opaqueredirect' || (r.status >= 300 && r.status < 400)))
      .catch(() => setSsoReady(false))
  }, [])

  const submit = async () => {
    if (!username.trim()) return message.warning(t('login.username', '用户名'))
    if (!password) return message.warning(t('login.passwordPlaceholder', '请输入密码'))
    setLoading(true)
    try {
      const { token, userId } = await api.login(username.trim(), password)
      userStore.set(username.trim()) // shown in profile / creator columns (backend has no /me yet)
      if (userId) userIdStore.set(userId) // created_by identity; "created by me" filters match on it
      login(token)
      message.success(t('login.ok', '登录成功'))
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('login.fail', '登录失败')}:${e.status}` : t('login.fail', '登录失败'))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="login-page">
      <div className={`login-card${mode === 'sso' ? ' mode-sso' : ''}`}>
        {/* Password form (left when active). */}
        <section className="login-panel login-panel-admin">
          <div className="login-panel-inner">
            <h1>{t('login.adminTitle', '账号密码登录')}</h1>
            <p className="login-panel-sub">{t('login.adminSub', '使用本地账号密码登录')}</p>
            <form
              className="login-form"
              onSubmit={(e) => {
                e.preventDefault()
                submit()
              }}
            >
              <Input
                size="large"
                prefix={<UserOutlined style={{ color: 'var(--text-3)' }} />}
                placeholder={t('login.username', '用户名')}
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete="username"
                autoFocus
              />
              <Input.Password
                size="large"
                prefix={<LockOutlined style={{ color: 'var(--text-3)' }} />}
                placeholder={t('login.passwordPlaceholder', '请输入密码')}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="current-password"
              />
              <button type="submit" className="login-btn" disabled={loading}>
                {loading ? t('login.loading', '登录中…') : t('login.submit', '登录')}
              </button>
            </form>
          </div>
        </section>

        {/* SSO panel (slides in from the right). */}
        <section className="login-panel login-panel-sso">
          <div className="login-panel-inner">
            <h1>{t('login.ssoTitle', '第三方登录')}</h1>
            <p className="login-panel-sub">
              {ssoReady ? t('login.ssoSub', '使用单点登录，免去记忆密码') : t('login.ssoNotConfigured', '未配置 OIDC 单点登录,请联系管理员')}
            </p>
            <button
              type="button"
              className="login-btn"
              disabled={!ssoReady}
              onClick={() => {
                window.location.href = `/auth/oidc/${OIDC_PROVIDER}/authorize`
              }}
            >
              {t('login.ssoButton', '单点登录 (SSO)')}
            </button>
          </div>
        </section>

        {/* Welcome overlay covers the inactive form and hosts the mode toggle. */}
        <div className="login-overlay-wrap">
          <div className="login-overlay">
            <div className="login-overlay-side left">
              <div className="login-overlay-inner">
                <h2>{t('login.welcomeBack', '欢迎回来')}</h2>
                <p>{t('login.welcomeBackSub', '本地账号请使用账号密码登录')}</p>
                <button type="button" className="login-btn-outline" onClick={() => setMode('admin')}>
                  {t('login.adminTitle', '账号密码登录')}
                </button>
              </div>
            </div>
            <div className="login-overlay-side right">
              <div className="login-overlay-inner">
                <h2>{t('login.hello', '你好，朋友')}</h2>
                <p>{t('login.helloSub', '用第三方登录，免去记忆密码的烦恼')}</p>
                <button type="button" className="login-btn-outline" onClick={() => setMode('sso')}>
                  {t('login.ssoTitle', '第三方登录')}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Brand mark floats above the active panel and slides with it. The mode
          key remounts the mark so the strata entrance replays on each switch,
          mirrored to match the travel direction. */}
      <div className={`login-logo${mode === 'sso' ? ' on-right' : ''}`}>
        <AnimatedLogo key={mode} size={104} className={mode === 'admin' && switched ? 'reverse' : undefined} />
        <span className="login-brand">Shepherd</span>
        <span className="login-slogan">{t('login.subtitle', 'Shepherd 将 AI 的一次性交付，转化为组织的长期软件工程资产。')}</span>
      </div>
    </div>
  )
}
