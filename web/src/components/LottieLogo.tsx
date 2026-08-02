import { useEffect, useRef } from 'react'

/** Plays a Lottie animation (default: the bundled /logo-animation.json). lottie-web is loaded lazily. */
export default function LottieLogo({ size = 96, src = '/logo-animation.json', loop = true }: { size?: number; src?: string; loop?: boolean }) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    let alive = true
    let anim: { destroy: () => void } | null = null
    import('lottie-web')
      .then(({ default: lottie }) => {
        if (!alive || !ref.current) return
        anim = lottie.loadAnimation({ container: ref.current, renderer: 'svg', loop, autoplay: true, path: src })
      })
      .catch(() => undefined)
    return () => {
      alive = false
      anim?.destroy()
    }
  }, [src, loop])
  return <div ref={ref} style={{ width: size, height: size }} aria-hidden />
}
