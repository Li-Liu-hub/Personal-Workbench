import { useEffect, useRef, useState } from 'react'
import { Alert, Button, Input, Select, Space, Table, Tag } from 'antd'

type RecordEntry = { id: string; tags: string[] }
type TextKey<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T] & string
export type RecordField<T> = { key: TextKey<T>; title: string; multiline?: boolean }

// Keep edits outside the filtered table so searching and pagination never discard input.
export default function EditableRecordTable<T extends RecordEntry>({ items, fields, tags, label, onSave, onDirty, actions, scrollContainer }: {
  items: T[]
  fields: RecordField<T>[]
  tags: string[]
  label: (item: T) => string
  onSave: (item: T) => Promise<void>
  onDirty: (dirty: boolean) => void
  actions: (item: T, editing: boolean) => React.ReactNode
  scrollContainer?: React.RefObject<HTMLDivElement | null>
}) {
  const [drafts, setDrafts] = useState<Record<string, T>>({})
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const saving = useRef(false)
  const mounted = useRef(true)
  const count = Object.keys(drafts).length
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => { onDirty(count > 0) }, [count, onDirty])
  function update(item: T, patch: Partial<T>) {
    setDrafts(current => ({ ...current, [item.id]: { ...(current[item.id] ?? item), ...patch } }))
  }
  function discard(id?: string) {
    if (!window.confirm(id ? '放弃这一行未保存的修改吗？' : '放弃所有行未保存的修改吗？')) return
    setDrafts(current => { if (!id) return {}; const next = { ...current }; delete next[id]; return next })
    setError('')
  }
  async function save(entries: T[]) {
    if (saving.current) return
    saving.current = true
    setBusy(true)
    setError('')
    try {
      for (const entry of entries) {
        await onSave(entry)
        if (!mounted.current) break
        setDrafts(current => { const next = { ...current }; delete next[entry.id]; return next })
      }
    } catch (e) {
      if (mounted.current) setError(`保存失败，未成功保存的输入已保留：${String(e)}`)
    } finally {
      saving.current = false
      if (mounted.current) setBusy(false)
    }
  }
  return <>
    {error && <Alert type="error" message={error} closable onClose={() => setError('')} />}
    {count > 0 && <Space style={{ marginBottom: 12 }} wrap><span>有 {count} 行待保存（包含筛选外的行）</span><Button type="primary" loading={busy} onClick={() => void save(Object.values(drafts))}>保存全部修改</Button><Button disabled={busy} onClick={() => discard()}>放弃全部修改</Button></Space>}
    <Table<T> rowKey="id" dataSource={items} pagination={{ pageSize: 30 }} scroll={{ x: 'max-content' }}
      sticky={scrollContainer ? { getContainer: () => scrollContainer.current ?? window } : false} columns={[
      ...fields.map(({ key, title, multiline }) => ({ key, title, render: (_: unknown, item: T) => {
        const editor = { 'aria-label': `${label(item)}${title}`, value: String((drafts[item.id] ?? item)[key] ?? ''), disabled: busy,
          onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => update(item, { [key]: e.target.value } as Partial<T>) }
        return multiline ? <Input.TextArea {...editor} autoSize={{ minRows: 1, maxRows: 4 }} style={{ minWidth: 160 }} /> : <Input {...editor} style={{ minWidth: 96 }} />
      } })),
      { key: 'tags', title: '分类', render: (_: unknown, item: T) => <Select mode="multiple" aria-label={`${label(item)}分类`} style={{ minWidth: 120 }} disabled={busy} value={(drafts[item.id] ?? item).tags} onChange={value => update(item, { tags: value } as Partial<T>)} options={tags.map(tag => ({ label: tag, value: tag }))} /> },
      { key: 'actions', title: '操作', render: (_: unknown, item: T) => <Space wrap>{drafts[item.id] ? <><Tag color="orange">未保存</Tag><Button loading={busy} onClick={() => void save([drafts[item.id]])}>保存行</Button><Button disabled={busy} onClick={() => discard(item.id)}>取消修改</Button></> : actions(item, busy || count > 0)}</Space> },
    ]} />
  </>
}
