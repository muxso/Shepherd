export async function copyToClipboard(text: string, container?: HTMLElement | null): Promise<boolean> {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text)
      return true
    } catch {
      // Try the legacy clipboard path when the browser denies this write.
    }
  }

  const focused = document.activeElement
  const input = document.createElement('textarea')
  input.value = text
  input.readOnly = true
  input.style.cssText = 'position:fixed;top:0;left:0;opacity:0;pointer-events:none'
  // Keep the selection inside the active dialog's focus trap.
  const host = container?.isConnected ? container : focused?.closest('[role="dialog"]') ?? document.body
  host.appendChild(input)
  try {
    input.focus()
    input.select()
    return document.execCommand('copy')
  } catch {
    return false
  } finally {
    input.remove()
    if (focused instanceof HTMLElement) focused.focus({ preventScroll: true })
  }
}
