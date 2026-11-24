import { useI18n } from '../../i18n'
import { useRef, useState } from 'react'
import { Button } from 'antd'

function captureTable(table: HTMLTableElement) {
  const bounds = table.getBoundingClientRect()
  const padding = 14
  const width = Math.ceil(bounds.width) + padding * 2
  const height = Math.ceil(bounds.height) + padding * 2
  const scale = Math.min(2, 16384 / Math.max(width, height), Math.sqrt(16_000_000 / (width * height)))
  const canvas = document.createElement('canvas')
  canvas.width = Math.max(1, Math.floor(width * scale))
  canvas.height = Math.max(1, Math.floor(height * scale))
  const context = canvas.getContext('2d')
  if (!context) throw new Error('Canvas unavailable')
  context.scale(scale, scale)
  context.translate(padding, padding)
  const style = getComputedStyle(table)
  context.fillStyle = style.getPropertyValue('--panel').trim() || '#fff'
  context.fillRect(-padding, -padding, width, height)
  const spacing = parseFloat(style.borderSpacing) || 0

  for (const row of Array.from(table.rows)) {
    for (const cell of Array.from(row.cells)) {
      const rect = cell.getBoundingClientRect()
      const cellStyle = getComputedStyle(cell)
      const left = cellStyle.position === 'sticky' ? spacing : rect.left - bounds.left
      const shift = left - rect.left
      const top = rect.top - bounds.top
      context.save()
      context.beginPath()
      context.rect(left, top, rect.width, rect.height)
      context.clip()
      for (const panel of [cell, ...Array.from(cell.querySelectorAll('.yx-activity-cell, .yx-activity-bug'))]) {
        const box = panel.getBoundingClientRect()
        const panelStyle = getComputedStyle(panel)
        const color = panelStyle.backgroundColor
        if (color === 'transparent' || color === 'rgba(0, 0, 0, 0)') continue
        context.fillStyle = color
        context.fillRect(box.left + shift, box.top - bounds.top, box.width, box.height)
      }
      const text = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT)
      for (let node = text.nextNode(); node; node = text.nextNode()) {
        if (!node.textContent?.trim() || !node.parentElement) continue
        const range = document.createRange()
        range.selectNodeContents(node)
        const box = range.getBoundingClientRect()
        if (!box.width || !box.height) continue
        const font = getComputedStyle(node.parentElement)
        context.font = `${font.fontWeight} ${font.fontSize} ${font.fontFamily}`
        context.fillStyle = font.color
        context.textBaseline = 'middle'
        context.fillText(node.textContent.trim(), box.left + shift, box.top - bounds.top + box.height / 2)
      }
      context.restore()
    }
  }
  return canvas
}

export default function ActivityScreenshot({ through, disabled }: { through?: string; disabled: boolean }) {
  const { t } = useI18n()
  const anchor = useRef<HTMLSpanElement>(null)
  const [saving, setSaving] = useState(false)
  const [failed, setFailed] = useState(false)
  const save = async () => {
    const table = anchor.current?.closest('.yx-activity-chart')?.querySelector<HTMLTableElement>('.yx-activity-matrix')
    if (!table) return
    setSaving(true)
    setFailed(false)
    try {
      await document.fonts.ready
      const canvas = captureTable(table)
      const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/png'))
      if (!blob) throw new Error('PNG export failed')
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      link.href = url
      link.download = `成员热力图-${through ?? '截图'}.png`
      link.click()
      window.setTimeout(() => URL.revokeObjectURL(url), 1000)
    } catch {
      setFailed(true)
    } finally {
      setSaving(false)
    }
  }
  return <span ref={anchor}>
    <Button size="small" loading={saving} disabled={disabled} onClick={save}>{t("engineering.exportScreenshot")}</Button>
    {failed && <span role="status">{t("engineering.couldNotExportTheScreenshot")}</span>}
  </span>
}
