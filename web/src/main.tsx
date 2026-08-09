import React, { useEffect } from 'react'
import ReactDOM from 'react-dom/client'
import { App as AntApp, ConfigProvider } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import enUS from 'antd/locale/en_US'
import jaJP from 'antd/locale/ja_JP'
import koKR from 'antd/locale/ko_KR'
import frFR from 'antd/locale/fr_FR'
import deDE from 'antd/locale/de_DE'
import esES from 'antd/locale/es_ES'
import ptBR from 'antd/locale/pt_BR'
import ruRU from 'antd/locale/ru_RU'
import itIT from 'antd/locale/it_IT'
import trTR from 'antd/locale/tr_TR'
import viVN from 'antd/locale/vi_VN'
import thTH from 'antd/locale/th_TH'
import idID from 'antd/locale/id_ID'
import nlNL from 'antd/locale/nl_NL'
import plPL from 'antd/locale/pl_PL'
import arEG from 'antd/locale/ar_EG'
import { BrowserRouter } from 'react-router-dom'
import { AppProvider } from './context'
import { LangProvider, type Lang } from './i18n'
import { ThemeModeProvider } from './themeMode'
import { themeFor } from './theme'
import { bindFeedback } from './feedback'
import App from './App'
import './index.css'

// Ant Design component locale per app language. Falls back to enUS for safety.
const ANTD_LOCALE: Record<Lang, typeof enUS> = {
  zh: zhCN,
  en: enUS,
  ja: jaJP,
  ko: koKR,
  fr: frFR,
  de: deDE,
  es: esES,
  pt: ptBR,
  ru: ruRU,
  it: itIT,
  tr: trTR,
  vi: viVN,
  th: thTH,
  id: idID,
  nl: nlNL,
  pl: plPL,
  ar: arEG,
}

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
            <ConfigProvider locale={ANTD_LOCALE[lang]} direction={lang === 'ar' ? 'rtl' : 'ltr'} theme={themeFor(mode)}>
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
