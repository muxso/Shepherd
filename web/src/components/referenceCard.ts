/** Measure the font used by the card labels; CJK, Latin and emoji have different widths. */
let context: CanvasRenderingContext2D | null | undefined
export function referenceCard(name: string, hub = false) {
  if (context === undefined) context = document.createElement('canvas').getContext('2d')
  const fontSize = hub ? 11 : 12.5
  if (context) context.font = `500 ${fontSize}px system-ui, sans-serif`
  const measure = (s: string) => context?.measureText(s).width ?? Array.from(s).length * fontSize
  const padding = hub ? 32 : 42
  const maxText = 280 - padding
  const lines: string[] = []
  let line = ''
  for (const char of Array.from(name)) {
    if (char === '\n') { lines.push(line); line = ''; continue }
    if (line && measure(line + char) > maxText) { lines.push(line); line = '' }
    line += char
  }
  lines.push(line)
  return {
    lines,
    width: Math.max(hub ? 144 : 104, Math.ceil(Math.max(...lines.map(measure))) + padding),
    height: Math.max(hub ? 54 : 38, lines.length * 18 + (hub ? 36 : 20)),
  }
}
