import { useLayoutEffect, useState, type RefObject } from 'react'

// Measure the unzoomed layout width of the workspace, not the window.
export default function useWideLayout(ref: RefObject<HTMLElement | null>, minWidth = 800) {
  const [wide, setWide] = useState(() => window.innerWidth >= minWidth + 204)
  useLayoutEffect(() => {
    const element = ref.current
    if (!element) return
    const update = () => setWide(element.clientWidth ? element.clientWidth >= minWidth : window.innerWidth >= minWidth + 204)
    update()
    const observer = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(update) : null
    observer?.observe(element)
    window.addEventListener('resize', update)
    return () => {
      observer?.disconnect()
      window.removeEventListener('resize', update)
    }
  }, [ref, minWidth])
  return wide
}
