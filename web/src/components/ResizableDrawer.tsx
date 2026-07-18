import { useCallback, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { Drawer, type DrawerProps } from 'antd'

const MIN_WIDTH = 360
const EDGE_GAP = 64

/**
 * Drop-in Drawer replacement: right-placed drawers get a grab handle on the
 * left edge for drag-resizing. Other placements pass through unchanged.
 */
export default function ResizableDrawer({ width, children, ...rest }: DrawerProps & { children?: ReactNode }) {
  const [w, setW] = useState<number | string | undefined>(undefined)
  const [host, setHost] = useState<HTMLElement | null>(null)
  const resizable = !rest.placement || rest.placement === 'right'

  // A hidden probe inside the body locates this drawer's content element so
  // the handle can be portaled to the content edge (unaffected by body scroll).
  const probeRef = useCallback((el: HTMLDivElement | null) => {
    setHost(el ? (el.closest('.ant-drawer-content') as HTMLElement | null) : null)
  }, [])

  const startDrag = (e: React.PointerEvent) => {
    e.preventDefault()
    const startX = e.clientX
    const startW = host?.getBoundingClientRect().width ?? (typeof w === 'number' ? w : MIN_WIDTH)
    const move = (ev: PointerEvent) => {
      const next = Math.min(window.innerWidth - EDGE_GAP, Math.max(MIN_WIDTH, startW + (startX - ev.clientX)))
      setW(next)
    }
    const up = () => {
      document.removeEventListener('pointermove', move)
      document.removeEventListener('pointerup', up)
      document.body.style.userSelect = ''
      document.body.style.cursor = ''
    }
    document.addEventListener('pointermove', move)
    document.addEventListener('pointerup', up)
    document.body.style.userSelect = 'none'
    document.body.style.cursor = 'col-resize'
  }

  return (
    <Drawer {...rest} width={w ?? width}>
      {resizable && <div ref={probeRef} style={{ display: 'none' }} />}
      {children}
      {resizable && host && createPortal(<div className="ms-drawer-resizer" onPointerDown={startDrag} />, host)}
    </Drawer>
  )
}
