// Projex rich text is a JSON envelope, not Markdown. Keep the stored source
// intact and extract its HTML representation only when displaying it.
export function richTextHtml(value: string): string | null {
  if (!value.trimStart().startsWith('{')) return null
  try {
    const content: unknown = JSON.parse(value)
    if (content && typeof content === 'object' && !Array.isArray(content) &&
      'htmlValue' in content && typeof content.htmlValue === 'string') {
      return content.htmlValue
    }
  } catch {
    // Ordinary Markdown (including incomplete JSON examples) stays unchanged.
  }
  return null
}
