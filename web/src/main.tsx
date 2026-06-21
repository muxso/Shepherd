import React, { useEffect } from 'react'
import ReactDOM from 'react-dom/client'
import { App as AntApp, ConfigProvider } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import enUS from 'antd/locale/en_US'
import { BrowserRouter } from 'react-router-dom'
import { AppProvider } from './context'
import { LangProvider } from './i18n'
import { bindFeedback } from './feedback'
import App from './App'
import './index.css'

// 把 <App> 提供的 message/modal 实例注入全局桥接,供静态调用走主题/context。
function FeedbackBridge() {
  const { message, modal } = AntApp.useApp()
  useEffect(() => {
    bindFeedback(message, modal)
  }, [message, modal])
  return null
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <LangProvider>
      {(lang) => (
        <ConfigProvider locale={lang === 'en' ? enUS : zhCN} theme={{ token: { colorPrimary: '#7c3aed' } }}>
          <AntApp component={false}>
            <FeedbackBridge />
            <BrowserRouter>
              <AppProvider>
                <App />
              </AppProvider>
            </BrowserRouter>
          </AntApp>
        </ConfigProvider>
      )}
    </LangProvider>
  </React.StrictMode>,
)
