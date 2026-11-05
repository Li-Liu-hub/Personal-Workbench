import { useEffect } from 'react'

export default function useUnsavedChanges(dirty: boolean, report?: (dirty: boolean) => void) {
  useEffect(() => { report?.(dirty) }, [dirty, report])
  useEffect(() => {
    const guard = (event: BeforeUnloadEvent) => {
      if (dirty) { event.preventDefault(); event.returnValue = '' }
    }
    window.addEventListener('beforeunload', guard)
    return () => window.removeEventListener('beforeunload', guard)
  }, [dirty])
}
