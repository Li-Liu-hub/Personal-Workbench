import { useState } from 'react'
import { Alert, Button, Input, Modal, Space } from 'antd'

type Props = {
  open: boolean
  tags: string[]
  onClose: () => void
  onCreate: (name: string) => Promise<boolean | void> | boolean | void
  onRename: (original: string, name: string) => Promise<boolean | void> | boolean | void
  onDelete: (name: string) => Promise<boolean | void> | boolean | void
  error?: string
  title?: string
  noun?: string
  deleteConfirm?: (name: string) => string
}

// Shared category/tag manager so every page uses the same antd dialog and validation flow.
export default function TagManagerModal({ open, tags, onClose, onCreate, onRename, onDelete, error = '', title, noun = '分类', deleteConfirm }: Props) {
  const [newTag, setNewTag] = useState('')
  const [busy, setBusy] = useState(false)
  async function run(work: () => Promise<boolean | void> | boolean | void) {
    if (busy) return false
    setBusy(true)
    try { return await work() } finally { setBusy(false) }
  }
  async function create() {
    const name = newTag.trim()
    if (!name) return
    const result = await run(() => onCreate(name))
    if (result !== false) setNewTag('')
  }
  async function rename(tag: string) {
    const name = window.prompt(`修改${noun}名称`, tag)
    if (!name || name === tag) return
    await run(() => onRename(tag, name))
  }
  async function remove(tag: string) {
    const message = deleteConfirm ? deleteConfirm(tag) : `删除${noun}“${tag}”？`
    if (!window.confirm(message)) return
    await run(() => onDelete(tag))
  }
  return <Modal title={title ?? `管理${noun}`} open={open} onCancel={onClose} footer={<Button onClick={onClose}>完成</Button>} afterClose={() => setNewTag('')} destroyOnHidden>
    {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }}/>}
    <Space.Compact style={{ width: '100%', marginBottom: 16 }}>
      <Input aria-label={`${noun}名称`} value={newTag} onChange={e => setNewTag(e.target.value)} placeholder={`输入${noun}名称`} onPressEnter={() => void create()}/>
      <Button type="primary" loading={busy} onClick={() => void create()}>添加</Button>
    </Space.Compact>
    {tags.map(tag => <div className="tag-row" key={tag}><span>{tag}</span><Space><Button type="text" disabled={busy} onClick={() => void rename(tag)}>重命名</Button><Button type="text" disabled={busy} danger onClick={() => void remove(tag)}>删除</Button></Space></div>)}
    {!tags.length && <p className="tag-empty">还没有{noun}，可在这里添加。</p>}
  </Modal>
}