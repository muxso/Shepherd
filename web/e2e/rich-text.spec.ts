import { test, expect } from '@playwright/test'
import { richTextHtml } from '../src/components/richTextContent'

test('extracts Projex HTML without leaking jsonML or altering ordinary Markdown/JSON', () => {
  const html = '<article><p>仅tpms工单打七折</p><p><strong>测试范围</strong></p><table><tr><td>预期</td></tr></table><img src="https://example.com/a.png"></article>'
  expect(richTextHtml(JSON.stringify({ htmlValue: html, jsonMLValue: ['root', {}, 'internal'] }))).toBe(html)
  expect(richTextHtml('{"htmlValue":""}')).toBe('')
  for (const value of ['# 普通需求\n\n- 步骤', '<p>已有HTML</p>', '{"example":"JSON示例"}', '{"htmlValue":23}', '{unfinished']) {
    expect(richTextHtml(value)).toBeNull()
  }
})
