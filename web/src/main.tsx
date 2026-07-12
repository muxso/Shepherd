import React, { useEffect } from 'react'
import ReactDOM from 'react-dom/client'
import { App as AntApp, ConfigProvider } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import enUS from 'antd/locale/en_US'
import { BrowserRouter } from 'react-router-dom'
import { AppProvider } from './context'
import { LangProvider } from './i18n'
import { ThemeModeProvider } from './themeMode'
import { themeFor } from './theme'
import { bindFeedback } from './feedback'
import App from './App'
import './index.css'

// Inject the message/modal instances provided by <App> into the global bridge so static calls get theme/context.
function FeedbackBridge() {
  const { message, modal } = AntApp.useApp()
  useEffect(() => {
    bindFeedback(message, modal)
  }, [message, modal])
  return null
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ThemeModeProvider>
      {(mode) => (
        <LangProvider>
          {(lang) => (
            <ConfigProvider locale={lang === 'en' ? enUS : zhCN} theme={themeFor(mode)}>
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
      )}
    </ThemeModeProvider>
  </React.StrictMode>,
)
