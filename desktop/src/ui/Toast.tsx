import { useEffect, useState } from 'react'

/**
 * Status toast with a graceful exit: the message fades/slides out instead of
 * vanishing, and a new message re-enters with the rise animation.
 */
export default function Toast({ message }: { message: string }) {
  const [shown, setShown] = useState(message)
  const [leaving, setLeaving] = useState(false)
  useEffect(() => {
    if (message) {
      setShown(message)
      setLeaving(false)
      return
    }
    if (!shown) return
    setLeaving(true)
    const timer = window.setTimeout(() => { setShown(''); setLeaving(false) }, 180)
    return () => window.clearTimeout(timer)
  }, [message, shown])
  if (!shown) return null
  return <div key={shown} className={`toast ${leaving ? 'leaving' : ''}`} role="status">✓ {shown}</div>
}
