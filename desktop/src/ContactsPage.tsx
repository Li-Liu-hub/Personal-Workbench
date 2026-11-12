import TagManagerModal from './TagManagerModal'
import useUnsavedChanges from './useUnsavedChanges'
import ChatPreview from './ChatPreview'
import CollectionShell from './ui/CollectionShell'
import { CollectionFilters, CollectionRow } from './ui/CollectionList'
import DetailHeader from './ui/DetailHeader'
import EmptyState from './ui/EmptyState'
import PinButton from './ui/PinButton'
import Toast from './ui/Toast'
import useWideLayout from './ui/useWideLayout'
import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Input, Modal, Select } from 'antd'
import { ContactsOutlined, CopyOutlined, DeleteOutlined, MessageOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import type { FocusState } from './navigation'

type Chat = { id: string; title: string; format: string; text: string; sheets: { name: string; rows: string[][] }[] }
type Contact = { id: string; name: string; school: string; major: string; work: string; phone: string; national_id: string; address: string; gender: string; birthday: string; hobbies: string; chat_notes: string; chats: Chat[] }
type ContactFields = Omit<Contact, 'chats'>
type Draft = { fields: ContactFields; tags: string[]; snapshot: string }
type Data = { items: Contact[]; tags: string[]; assignments: { id: string; tags: string[] }[]; favorites: string[]; view: 'grid' | 'list'; extra: boolean }
const empty: Data = { items: [], tags: [], assignments: [], favorites: [], view: 'grid', extra: false }
const blank = (): ContactFields => ({ id: crypto.randomUUID(), name: '', school: '', major: '', work: '', phone: '', national_id: '', address: '', gender: '', birthday: '', hobbies: '', chat_notes: '' })
function contactFields({ chats: _chats, ...fields }: Contact): ContactFields { return fields }
const basicFields: readonly { key: keyof ContactFields; label: string; placeholder?: string; copy?: boolean }[] = [
  { key: 'name', label: '姓名' }, { key: 'gender', label: '性别' },
  { key: 'birthday', label: '生日', placeholder: 'YYYY-MM-DD' }, { key: 'phone', label: '电话', copy: true },
  { key: 'school', label: '学校' }, { key: 'major', label: '专业' },
  { key: 'work', label: '工作' }, { key: 'national_id', label: '证件号码', copy: true },
]

export default function ContactsPage({ setWorkspaceDirty, focus }: {
  setWorkspaceDirty?: (value: boolean) => void
  focus?: FocusState | null
}) {
  const [data, setData] = useState<Data>(empty)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [detailDismissed, setDetailDismissed] = useState(false)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [manage, setManage] = useState(false)
  const [chat, setChat] = useState<Chat | null>(null)
  const [photo, setPhoto] = useState<string | null>(null)
  const [photoRevision, setPhotoRevision] = useState(0)
  const [attachmentBusy, setAttachmentBusy] = useState(false)
  const [busy, setBusy] = useState(false)
  const [loading, setLoading] = useState(true)
  const handledFocus = useRef<number | null>(null)
  const layout = useRef<HTMLDivElement>(null)
  const wide = useWideLayout(layout)

  const stored = data.items.find(item => item.id === selectedId)
  const fields = draft?.fields ?? (stored ? contactFields(stored) : null)
  const isNew = !!draft && !stored
  const dirty = !!draft && JSON.stringify([draft.fields, draft.tags]) !== draft.snapshot
  useUnsavedChanges(dirty, setWorkspaceDirty)
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2400); return () => window.clearTimeout(timer) }, [notice])

  const tags = draft?.tags ?? data.assignments.find(item => item.id === selectedId)?.tags ?? []
  const visible = useMemo(() => data.items.filter(item => {
    const itemTags = data.assignments.find(assignment => assignment.id === item.id)?.tags || []
    return (filter === 'all' || (filter === 'untagged' ? !itemTags.length : itemTags.includes(filter)))
      && [item.name, item.phone, item.work, item.school, item.major, item.chat_notes, ...item.chats.flatMap(record => [record.title, record.text]), ...itemTags]
        .some(value => value.toLowerCase().includes(query.trim().toLowerCase()))
  }).sort((a, b) => Number(data.favorites.includes(b.id)) - Number(data.favorites.includes(a.id))), [data, filter, query])

  async function refresh() {
    try {
      setData(await invoke<Data>('contact_data'))
      setError('')
    } catch (e) {
      setError(`读取联系人失败：${String(e)}`)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { void refresh() }, [])

  useEffect(() => {
    if (!wide || dirty || isNew || busy || attachmentBusy) return
    if (selectedId && visible.some(item => item.id === selectedId)) return
    if (detailDismissed) return
    setDraft(null)
    setSelectedId(visible[0]?.id ?? null)
  }, [visible, dirty, isNew, busy, attachmentBusy, selectedId, detailDismissed, wide])

  useEffect(() => {
    if (!focus?.id || handledFocus.current === focus.nonce) return
    const item = data.items.find(record => record.id === focus.id)
    if (item) {
      handledFocus.current = focus.nonce
      setDraft(null)
      setQuery('')
      setFilter('all')
      setSelectedId(item.id)
      setDetailDismissed(false)
    }
  }, [focus?.nonce, focus?.id, data.items])

  useEffect(() => { setDetailDismissed(false) }, [query, filter])

  const contactId = stored?.id
  useEffect(() => {
    let cancelled = false
    setPhoto(null)
    if (contactId) {
      void invoke<string | null>('contact_photo', { id: contactId })
        .then(value => { if (!cancelled) setPhoto(value) })
        .catch(() => { if (!cancelled) setPhoto(null) })
    }
    return () => { cancelled = true }
  }, [contactId, photoRevision])

  function select(item: Contact) {
    if (busy || attachmentBusy || item.id === selectedId) return
    if (dirty && !window.confirm('联系人资料有未保存的修改，确定切换吗？')) return
    setDraft(null)
    setSelectedId(item.id)
    setDetailDismissed(false)
    setError('')
  }

  function update(patch: Partial<ContactFields>, nextTags = tags) {
    if (!fields) return
    setDraft({ fields: { ...fields, ...patch }, tags: nextTags, snapshot: draft?.snapshot ?? JSON.stringify([fields, tags]) })
  }

  function create() {
    if (busy || attachmentBusy || (dirty && !window.confirm('联系人资料有未保存的修改，确定新增吗？'))) return
    const item = blank()
    setDraft({ fields: item, tags: [], snapshot: JSON.stringify([item, []]) })
    setSelectedId(item.id)
    setDetailDismissed(false)
    setError('')
  }

  function closeDetail() {
    if (busy || attachmentBusy) return
    if (dirty && !window.confirm('联系人资料有未保存的修改，确定放弃吗？')) return
    setDraft(null)
    setSelectedId(null)
    setDetailDismissed(true)
    setError('')
  }

  function discard() {
    if (isNew) { closeDetail(); return }
    if (dirty && !window.confirm('联系人资料有未保存的修改，确定放弃吗？')) return
    setDraft(null)
    setError('')
  }

  async function save() {
    if (!draft || busy || attachmentBusy) return
    if (!draft.fields.name.trim()) { setError('请填写姓名'); return }
    setBusy(true)
    try {
      const entry = { ...draft.fields, name: draft.fields.name.trim(), chats: stored?.chats ?? [] }
      await invoke('contact_save_classified', { entry, tags: draft.tags })
      await refresh()
      setDraft(null)
      setSelectedId(entry.id)
      setNotice('联系人已保存')
      setError('')
    } catch (e) {
      setError(`保存失败：${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  async function action(command: string, args: Record<string, unknown>) {
    try {
      await invoke(command, args)
      await refresh()
      return true
    } catch (e) {
      setError(`操作失败：${String(e)}`)
      return false
    }
  }

  async function remove(item: Contact) {
    if (!window.confirm(`删除联系人“${item.name}”及其聊天记录？`)) return
    if (await action('contacts_delete', { id: item.id })) {
      if (selectedId === item.id) setSelectedId(null)
      setNotice('联系人已删除')
    }
  }

  async function createTag(name: string) { return action('contact_tag_create', { name }) }
  async function renameTag(original: string, name: string) {
    const ok = await action('contact_tag_rename', { original, name })
    if (ok && filter === original) setFilter(name)
    return ok
  }
  async function deleteTag(name: string) {
    const ok = await action('contact_tag_delete', { name })
    if (ok && filter === name) setFilter('all')
    return ok
  }

  async function importAttachment(command: string) {
    if (!stored || attachmentBusy || busy) return
    setAttachmentBusy(true)
    try {
      const result = await invoke<boolean | { added: number; skipped: number } | null>(command, { id: stored.id })
      if (result === null || result === false) return
      setNotice(typeof result === 'object' ? `已导入 ${result.added} 份聊天记录，跳过 ${result.skipped} 份重复记录` : '照片已导入')
      await refresh()
      if (command === 'contact_import_photo') setPhotoRevision(value => value + 1)
      setError('')
    } catch (e) {
      setError(`导入或刷新失败：${String(e)}`)
    } finally {
      setAttachmentBusy(false)
    }
  }

  async function removeAttachment(command: string, chatId?: string) {
    if (!stored || attachmentBusy || busy) return
    setAttachmentBusy(true)
    try {
      await invoke(command, { id: stored.id, ...(chatId ? { chatId } : {}) })
      await refresh()
      if (command === 'contact_remove_photo') setPhotoRevision(value => value + 1)
      setChat(null)
      setNotice('已移除')
      setError('')
    } catch (e) {
      setError(`移除失败：${String(e)}`)
    } finally {
      setAttachmentBusy(false)
    }
  }

  async function copyField(label: string, value: string) {
    if (!value) return
    try {
      await invoke('copy_value', { value })
      setNotice(`${label}已复制`)
      setError('')
    } catch {
      setError('复制失败，请检查剪贴板权限')
    }
  }

  const copyAction = (label: string, value: string) => <Button type="text" size="small" icon={<CopyOutlined/>}
    aria-label={`复制${label}`} title={`复制${label}`} disabled={!value} onClick={() => void copyField(label, value)}/>

  async function exportChat(record: Chat) {
    if (!stored) return
    try {
      const path = await invoke<string | null>('contact_export_chat', { id: stored.id, chatId: record.id })
      if (path) setNotice(`已导出：${path}`)
    } catch (e) {
      setError(`导出失败：${String(e)}`)
    }
  }

  const list = <>
    <div className="collection-list-head">
      <div className="collection-list-heading"><h1>联系人</h1>
        <span className="collection-count">{loading ? '读取中' : `${visible.length} 位`}</span>
      </div>
      <div className="collection-list-actions">
        <Button type="primary" icon={<PlusOutlined/>} onClick={create} disabled={busy || attachmentBusy}>添加联系人</Button>
        <Button type="text" onClick={() => { setError(''); setManage(true) }} disabled={dirty || busy || attachmentBusy}>管理分类</Button>
      </div>
    </div>
    {!fields && error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')} style={{ margin: '0 14px 10px' }}/>}
    <div className="collection-search">
      <Input aria-label="搜索联系人" prefix={<SearchOutlined/>} placeholder="搜索姓名、电话或聊天记录" value={query} onChange={event => setQuery(event.target.value)} allowClear/>
    </div>
    <CollectionFilters tags={data.tags} selected={filter} onChange={setFilter}/>
    <div className="collection-list" role="list" aria-label="联系人记录" aria-busy={loading}>
      {isNew && fields && <CollectionRow id={fields.id} title={fields.name || '新联系人'} subtitle="正在添加" current onSelect={() => setSelectedId(fields.id)}/>}
      {visible.map(item => <CollectionRow
        key={item.id}
        id={item.id}
        title={item.name}
        subtitle={[item.work, item.school, item.phone].filter(Boolean).join(' / ') || '未填写详细信息'}
        current={!isNew && selectedId === item.id}
        pinned={data.favorites.includes(item.id)}
        onSelect={() => select(item)}
      />)}
      {!visible.length && !isNew && <div className="collection-list-empty" role="status">
        <span>{loading ? '正在读取联系人…' : error && !data.items.length ? '暂时无法读取联系人。' : query || filter !== 'all' ? '没有找到匹配的联系人。' : '还没有联系人记录。'}</span>
        {!loading && (query || filter !== 'all') && <Button type="link" size="small" onClick={() => { setQuery(''); setFilter('all') }}>清除筛选</Button>}
        {!loading && error && !data.items.length && <Button size="small" onClick={() => { setLoading(true); void refresh() }}>重试</Button>}
      </div>}
    </div>
  </>

  const locked = busy || attachmentBusy
  const detail = !fields ? <div className="detail-empty">
    <EmptyState icon={<ContactsOutlined/>} title="从左侧选择一位联系人" description="资料、照片和聊天记录会在这里展开；也可以新建一位联系人。" actions={<Button type="primary" icon={<PlusOutlined/>} onClick={create}>添加联系人</Button>}/>
  </div> : <div className="detail-sheet" key={fields.id}>
    <DetailHeader
      title={isNew ? '新增联系人' : fields.name || '未命名联系人'}
      subtitle={dirty ? '有未保存的修改' : undefined}
      onBack={closeDetail}
      backLabel="返回联系人列表"
      actions={<div className="inline-actions">
        {stored && <PinButton pinned={data.favorites.includes(stored.id)} label={stored.name} disabled={locked} onClick={() => void action('contact_favorite', { id: stored.id, value: !data.favorites.includes(stored.id) })}/>}
        {stored && <Button type="text" danger icon={<DeleteOutlined/>} aria-label="删除联系人" disabled={locked || dirty} onClick={() => void remove(stored)}>删除</Button>}
        <Button onClick={discard} disabled={locked || (!dirty && !isNew)}>{isNew ? '取消' : '还原'}</Button>
        <Button type="primary" loading={busy} disabled={attachmentBusy || (!dirty && !isNew)} onClick={() => void save()}>保存联系人</Button>
      </div>}
    />
    {stored && <div className="contact-identity">
      {photo ? <img className="contact-photo" src={photo} alt={`${stored.name}的照片`}/> : <span className="contact-photo contact-photo--empty" aria-hidden="true">{Array.from(stored.name)[0] || '?'}</span>}
      <div className="inline-actions">
        <Button onClick={() => void importAttachment('contact_import_photo')} disabled={locked}>导入照片</Button>
        {photo && <Button onClick={() => void removeAttachment('contact_remove_photo')} disabled={locked}>移除照片</Button>}
        <Button icon={<MessageOutlined/>} onClick={() => void importAttachment('contact_import_chats')} disabled={locked}>导入聊天记录</Button>
      </div>
    </div>}
    <div className="detail-form">
      {error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')}/>}
      <div className="form-grid">
        {basicFields.map(({ key, label, placeholder, copy }) => <div className="form-field" key={key}>
          <span>{label}{key === 'name' && <em> *</em>}</span>
          <Input aria-label={label} aria-required={key === 'name'} autoFocus={isNew && key === 'name'} disabled={locked}
            value={fields[key]} onChange={event => update({ [key]: event.target.value })} placeholder={placeholder}
            suffix={copy ? copyAction(label, fields[key]) : undefined}/>
        </div>)}
      </div>
      <div className="form-field"><span>地址</span><Input aria-label="地址" value={fields.address} disabled={locked} onChange={event => update({ address: event.target.value })} suffix={copyAction('地址', fields.address)}/></div>
      <label className="form-field"><span>爱好</span><Input aria-label="爱好" value={fields.hobbies} disabled={locked} onChange={event => update({ hobbies: event.target.value })}/></label>
      <label className="form-field"><span>分类</span><Select mode="multiple" aria-label="分类" style={{ width: '100%' }} disabled={locked} value={tags} onChange={value => update({}, value)} options={data.tags.map(tag => ({ value: tag, label: tag }))} placeholder="选择已有分类"/></label>
      <label className="form-field"><span>聊天备注</span><Input.TextArea aria-label="聊天备注" rows={3} value={fields.chat_notes} disabled={locked} onChange={event => update({ chat_notes: event.target.value })}/></label>
    </div>
    {stored && <div className="detail-section">
      <div className="detail-section-title"><span>聊天记录</span><span>{stored.chats.length} 份</span></div>
      {stored.chats.length ? stored.chats.map(record => <div className="attachment-row" key={record.id}>
        <button type="button" className="attachment-open" onClick={() => setChat(record)}><MessageOutlined/><span>{record.title}</span></button>
        <div className="inline-actions">
          <Button type="text" size="small" onClick={() => void exportChat(record)}>导出原件</Button>
          <Button type="text" size="small" danger onClick={() => void removeAttachment('contact_remove_chat', record.id)} disabled={locked}>移除</Button>
        </div>
      </div>) : <p className="meta" style={{ padding: '10px 0' }}>还没有导入聊天记录。</p>}
    </div>}
  </div>

  return <div className="content collection-content" ref={layout}>
    <CollectionShell wide={wide} detailId={fields?.id} list={list} detail={detail}/>
    <Toast message={notice}/>
    <TagManagerModal open={manage} tags={data.tags} error={error} onClose={() => setManage(false)} onCreate={createTag} onRename={renameTag} onDelete={deleteTag} deleteConfirm={tag => `删除分类“${tag}”？联系人不会删除。`}/>
    <Modal title={chat?.title} width={860} open={!!chat} onCancel={() => setChat(null)} footer={<Button onClick={() => setChat(null)}>关闭</Button>}>{chat && <ChatPreview key={chat.id} chat={chat} onExport={() => void exportChat(chat)}/>}</Modal>
  </div>
}
