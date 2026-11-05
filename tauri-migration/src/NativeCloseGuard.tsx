import { useEffect, useRef, useState } from 'react'
import { Alert, Modal } from 'antd'
import { getCurrentWindow } from '@tauri-apps/api/window'

export default function NativeCloseGuard({ dirty }: { dirty: boolean }) {
  const dirtyRef = useRef(dirty)
  dirtyRef.current = dirty
  const [open, setOpen] = useState(false)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void getCurrentWindow().onCloseRequested(event => {
      if (dirtyRef.current) { event.preventDefault(); setOpen(true) }
    }).then(stop => { if (disposed) stop(); else unlisten = stop }).catch(e => {
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
    {error && <Alert type="error" message={error} />}
    <Modal title="仍有未保存的修改" open={open} okText="放弃修改并关闭" cancelText="继续编辑" confirmLoading={busy} onCancel={() => setOpen(false)} onOk={() => void close()}>
      <p>直接关闭可能丢失未保存的内容。返回编辑页面保存后再关闭，或放弃修改。</p>
    </Modal>
  </>
}
