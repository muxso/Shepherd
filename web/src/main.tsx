import React from 'react'
import ReactDOM from 'react-dom/client'
import { ConfigProvider } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import enUS from 'antd/locale/en_US'
import { BrowserRouter } from 'react-router-dom'
import { AppProvider } from './context'
import { LangProvider } from './i18n'
import App from './App'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <LangProvider>
      {(lang) => (
        <ConfigProvider locale={lang === 'en' ? enUS : zhCN} theme={{ token: { colorPrimary: '#7c3aed' } }}>
          <BrowserRouter>
            <AppProvider>
              <App />
            </AppProvider>
          </BrowserRouter>
        </ConfigProvider>
      )}
    </LangProvider>
  </React.StrictMode>,
)
