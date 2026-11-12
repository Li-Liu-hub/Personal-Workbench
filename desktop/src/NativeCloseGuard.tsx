import { lazy, Suspense, useEffect, useEffectEvent, useRef, useState } from 'react'
import { Alert } from 'antd'
import { getCurrentWindow, type CloseRequestedEvent } from '@tauri-apps/api/window'

const Modal = lazy(() => import('antd/es/modal'))

export default function NativeCloseGuard({ dirty, getWritingGuard }: {
  dirty: boolean; getWritingGuard?: () => (() => Promise<boolean>) | null
}) {
  const closing = useRef(false)
  const [open, setOpen] = useState(false)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const requestClose = useEffectEvent((event: CloseRequestedEvent) => {
    const flush = getWritingGuard?.()
    if (!flush) {
      if (dirty) { event.preventDefault(); setOpen(true) }
      return
    }
    event.preventDefault()
    if (closing.current) return
    closing.current = true
    return (async () => {
      setError('')
      try {
        if (await flush()) await getCurrentWindow().destroy()
        else setError('当前文档还未保存，窗口已保留。')
      } catch (e) {
        setError(`暂时无法关闭窗口：${String(e)}`)
      } finally {
        closing.current = false
      }
    })()
  })
  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void getCurrentWindow().onCloseRequested(requestClose).then(stop => { if (disposed) stop(); else unlisten = stop }).catch(e => {
      if (!disposed) setError(`关闭保护加载失败：${String(e)}`)
    })
    return () => { disposed = true; unlisten?.() }
  }, [])
  async function close() {
    setBusy(true)
    try { await getCurrentWindow().destroy() }
    catch (e) { setError(`关闭失败：${String(e)}`); setBusy(false) }
  }
  return <>
    {error && <Alert className="window-close-alert" type="error" showIcon closable message={error} onClose={() => setError('')}/>}
    {open && <Suspense fallback={null}>
      <Modal title="仍有未保存的修改" open okText="放弃修改并关闭" cancelText="继续编辑" confirmLoading={busy} onCancel={() => setOpen(false)} onOk={() => void close()}>
        <p>直接关闭可能丢失未保存的内容。返回编辑页面保存后再关闭，或放弃修改。</p>
      </Modal>
    </Suspense>}
  </>
}
