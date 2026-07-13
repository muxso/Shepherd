import { useMemo } from 'react'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import './MarkdownRenderer.css'

export interface MarkdownRendererProps {
  value?: string
  className?: string
}

export function MarkdownRenderer({ value, className }: MarkdownRendererProps) {
  const html = useMemo(() => {
    if (!value) return ''
    const raw = marked.parse(value, { async: false, gfm: true, breaks: true }) as string
    return DOMPurify.sanitize(raw, { USE_PROFILES: { html: true } })
  }, [value])

  return html ? (
    <div className={`markdown-renderer ${className || ''}`} dangerouslySetInnerHTML={{ __html: html }} />
  ) : (
    <span className="markdown-empty">—</span>
  )
}
