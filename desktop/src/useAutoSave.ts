import { useEffect, useRef } from 'react'

/** Debounce formal saves. A failed version is not retried until edited or explicitly saved. */
export default function useAutoSave(value: unknown, dirty: boolean, busy: boolean, save: () => Promise<void>) {
  const latestSave = useRef(save)
  latestSave.current = save
  const attempted = useRef('')
  const version = JSON.stringify(value)
  useEffect(() => {
    if (!dirty) { attempted.current = ''; return }
    if (!value || busy || attempted.current === version) return
    const timer = window.setTimeout(() => {
      attempted.current = version
      // The editor's save callback owns error presentation and preserves the unsaved content.
      void latestSave.current()
    }, 1500)
    return () => window.clearTimeout(timer)
  }, [version, dirty, busy, value])
}
