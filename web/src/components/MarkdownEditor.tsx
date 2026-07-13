import { useCallback, useRef, useState } from 'react'
import { Button, Tabs, Tooltip, Upload } from 'antd'
import { PictureOutlined } from '@ant-design/icons'
import { message } from '../feedback'
import { useI18n } from '../i18n'
import { MarkdownRenderer } from './MarkdownRenderer'
import './MarkdownEditor.css'

export interface MarkdownEditorProps {
  value?: string
  onChange?: (value: string) => void
  placeholder?: string
  /** Max image size in bytes; default 5MB. */
  maxImageSize?: number
}

export function MarkdownEditor({ value = '', onChange, placeholder, maxImageSize = 5 * 1024 * 1024 }: MarkdownEditorProps) {
  const { t } = useI18n()
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const [activeTab, setActiveTab] = useState('write')

  const insertText = useCallback((before: string, after: string = '') => {
    const el = textareaRef.current
    if (!el) return
    const start = el.selectionStart ?? 0
    const end = el.selectionEnd ?? 0
    const selected = value.slice(start, end)
    const next = value.slice(0, start) + before + selected + after + value.slice(end)
    onChange?.(next)
    requestAnimationFrame(() => {
      el.focus()
      const pos = start + before.length + selected.length
      el.setSelectionRange(pos, pos)
    })
  }, [value, onChange])

  const insertImage = useCallback((url: string, alt: string) => {
    insertText(`![${alt}](${url})`, '')
  }, [insertText])

  const readImage = useCallback((file: File) => {
    if (file.size > maxImageSize) {
      const mb = (maxImageSize / 1024 / 1024).toFixed(1)
      message.error(t('md.imageTooLarge', '图片超过 {size}MB').replace('{size}', mb))
      return
    }
    const reader = new FileReader()
    reader.onload = (e) => {
      const url = e.target?.result as string
      if (url) {
        insertImage(url, file.name.replace(/\.[^.]+$/, ''))
        setActiveTab('write')
      }
    }
    reader.readAsDataURL(file)
  }, [insertImage, maxImageSize, t])

  const handlePaste = useCallback((e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const items = e.clipboardData?.items
    if (!items) return
    for (const item of Array.from(items)) {
      if (item.type.startsWith('image/')) {
        const file = item.getAsFile()
        if (file) {
          e.preventDefault()
          readImage(file)
          return
        }
      }
    }
  }, [readImage])

  const handleUpload = useCallback((file: File) => {
    readImage(file)
    return false
  }, [readImage])

  const toolbar = [
    { label: 'H1', title: t('md.heading', '标题'), action: () => insertText('# ', '\n\n') },
    { label: 'H2', title: t('md.heading', '标题'), action: () => insertText('## ', '\n\n') },
    { label: 'B', title: t('md.bold', '粗体'), action: () => insertText('**', '**') },
    { label: 'I', title: t('md.italic', '斜体'), action: () => insertText('*', '*') },
    { label: '>', title: t('md.quote', '引用'), action: () => insertText('> ', '\n') },
    { label: '</>', title: t('md.code', '代码'), action: () => insertText('```\n', '\n```') },
    { label: 'Link', title: t('md.link', '链接'), action: () => insertText('[', '](url)') },
  ]

  const items = [
    {
      key: 'write',
      label: t('md.write', '编辑'),
      children: (
        <>
          <div className="markdown-editor-toolbar">
            {toolbar.map((btn, i) => (
              <Tooltip title={btn.title} key={i}>
                <Button size="small" onClick={btn.action}>{btn.label}</Button>
              </Tooltip>
            ))}
            <Tooltip title={t('md.uploadImage', '上传图片')}>
              <Upload accept="image/*" showUploadList={false} beforeUpload={handleUpload}>
                <Button size="small" icon={<PictureOutlined />}>{t('md.image', '图片')}</Button>
              </Upload>
            </Tooltip>
          </div>
          <textarea
            ref={textareaRef}
            className="markdown-editor-textarea"
            value={value}
            onChange={(e) => onChange?.(e.target.value)}
            onPaste={handlePaste}
            placeholder={placeholder}
            rows={8}
          />
        </>
      ),
    },
    {
      key: 'preview',
      label: t('md.preview', '预览'),
      children: (
        <div className="markdown-editor-preview">
          <MarkdownRenderer value={value} />
        </div>
      ),
    },
  ]

  return (
    <div className="markdown-editor">
      <Tabs activeKey={activeTab} onChange={setActiveTab} size="small" items={items} />
    </div>
  )
}
