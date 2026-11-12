import TagManagerModal from './TagManagerModal'
import useUnsavedChanges from './useUnsavedChanges'
import ChatPreview from './ChatPreview'
import CollectionShell from './ui/CollectionShell'
import { CollectionFilters, CollectionRow } from './ui/CollectionList'
import DetailHeader from './ui/DetailHeader'
import DetailField from './ui/DetailField'
import EmptyState from './ui/EmptyState'
import PinButton from './ui/PinButton'
import Toast from './ui/Toast'
import useWideLayout from './ui/useWideLayout'
import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Input, Modal, Select } from 'antd'
import { ContactsOutlined, CopyOutlined, DeleteOutlined, EditOutlined, MessageOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import type { FocusState } from './navigation'

type Chat = { id: string; title: string; format: string; text: string; sheets: { name: string; rows: string[][] }[] }
type Contact = { id: string; name: string; school: string; major: string; work: string; phone: string; national_id: string; address: string; gender: string; birthday: string; hobbies: string; chat_notes: string; chats: Chat[] }
type Data = { items: Contact[]; tags: string[]; assignments: { id: string; tags: string[] }[]; favorites: string[]; view: 'grid' | 'list'; extra: boolean }
const empty: Data = { items: [], tags: [], assignments: [], favorites: [], view: 'grid', extra: false }
const blank = (): Contact => ({ id: crypto.randomUUID(), name: '', school: '', major: '', work: '', phone: '', national_id: '', address: '', gender: '', birthday: '', hobbies: '', chat_notes: '', chats: [] })

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
  const [draft, setDraft] = useState<Contact | null>(null)
  const [draftTags, setDraftTags] = useState<string[]>([])
  const [snapshot, setSnapshot] = useState('')
  const [editing, setEditing] = useState(false)
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

  const dirty = editing && !!draft && JSON.stringify([draft, draftTags]) !== snapshot
  useUnsavedChanges(dirty, setWorkspaceDirty)
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2400); return () => window.clearTimeout(timer) }, [notice])

  const labels = (id: string) => data.assignments.find(item => item.id === id)?.tags || []
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
    if (!wide || editing) return
    if (selectedId && visible.some(item => item.id === selectedId)) return
    if (detailDismissed) return
    setSelectedId(visible[0]?.id ?? null)
  }, [visible, editing, selectedId, detailDismissed, wide])

  useEffect(() => {
    if (!focus?.id || handledFocus.current === focus.nonce) return
    const item = data.items.find(record => record.id === focus.id)
    if (item) {
      handledFocus.current = focus.nonce
      setEditing(false)
      setDraft(null)
      setQuery('')
      setFilter('all')
      setSelectedId(item.id)
      setDetailDismissed(false)
    }
  }, [focus?.nonce, focus?.id, data.items])

  useEffect(() => { setDetailDismissed(false) }, [query, filter])

  const selected = editing && draft ? draft : data.items.find(item => item.id === selectedId) || null
  const isNew = editing && !!draft && !data.items.some(item => item.id === draft.id)
  const selectedTags = selected ? (editing ? draftTags : labels(selected.id)) : []

  const contactId = selected?.id
  const contactExists = !!contactId && data.items.some(item => item.id === contactId)
  useEffect(() => {
    let cancelled = false
    setPhoto(null)
    if (contactId && contactExists) {
      void invoke<string | null>('contact_photo', { id: contactId })
        .then(value => { if (!cancelled) setPhoto(value) })
        .catch(() => { if (!cancelled) setPhoto(null) })
    }
    return () => { cancelled = true }
  }, [contactId, contactExists, photoRevision])

  function select(item: Contact) {
    if (editing && dirty && !window.confirm('联系人资料有未保存的修改，确定切换吗？')) return
    setEditing(false)
    setDraft(null)
    setSelectedId(item.id)
    setDetailDismissed(false)
    setError('')
  }

  function startEdit(item: Contact) {
    const tags = labels(item.id)
    setDraft({ ...item })
    setDraftTags([...tags])
    setSnapshot(JSON.stringify([item, tags]))
    setEditing(true)
    setSelectedId(item.id)
    setDetailDismissed(false)
    setError('')
  }

  function create() {
    const item = blank()
    setDraft({ ...item })
    setDraftTags([])
    setSnapshot(JSON.stringify([item, []]))
    setEditing(true)
    setSelectedId(item.id)
    setDetailDismissed(false)
    setError('')
  }

  function cancelEdit() {
    if (dirty && !window.confirm('联系人资料有未保存的修改，确定放弃吗？')) return
    setEditing(false)
    setDraft(null)
    setError('')
    if (!data.items.some(item => item.id === selectedId)) {
      setSelectedId(wide ? visible[0]?.id ?? null : null)
      if (!wide) setDetailDismissed(true)
    }
  }

  async function save() {
    if (!draft || busy) return
    if (!draft.name.trim()) { setError('请填写姓名'); return }
    setBusy(true)
    try {
      const entry = { ...draft, name: draft.name.trim() }
      await invoke('contact_save_classified', { entry, tags: draftTags })
      await refresh()
      setEditing(false)
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
    if (!selected || attachmentBusy) return
    setAttachmentBusy(true)
    try {
      const result = await invoke<boolean | { added: number; skipped: number } | null>(command, { id: selected.id })
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
    if (!selected || attachmentBusy) return
    setAttachmentBusy(true)
    try {
      await invoke(command, { id: selected.id, ...(chatId ? { chatId } : {}) })
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

  const copyAction = (label: string, value: string) => value
    ? <Button type="text" size="small" icon={<CopyOutlined/>} aria-label={`复制${label}`} title={`复制${label}`} onClick={() => void copyField(label, value)}/>
    : undefined

  async function exportChat(record: Chat) {
    if (!selected) return
    try {
      const path = await invoke<string | null>('contact_export_chat', { id: selected.id, chatId: record.id })
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
        <Button type="primary" icon={<PlusOutlined/>} onClick={create} disabled={editing}>添加联系人</Button>
        <Button type="text" onClick={() => { setError(''); setManage(true) }} disabled={editing}>管理分类</Button>
      </div>
    </div>
    {!selected && error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')} style={{ margin: '0 14px 10px' }}/>}
    <div className="collection-search">
      <Input aria-label="搜索联系人" prefix={<SearchOutlined/>} placeholder="搜索姓名、电话或聊天记录" value={query} onChange={event => setQuery(event.target.value)} allowClear/>
    </div>
    <CollectionFilters tags={data.tags} selected={filter} onChange={setFilter}/>
    <div className="collection-list" role="list" aria-label="联系人记录" aria-busy={loading}>
      {editing && isNew && draft && <CollectionRow id={draft.id} title={draft.name || '新联系人'} subtitle="正在添加" current onSelect={() => setSelectedId(draft.id)}/>}
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

  const detail = !selected ? <div className="detail-empty">
    <EmptyState icon={<ContactsOutlined/>} title="从左侧选择一位联系人" description="资料、照片和聊天记录会在这里展开；也可以新建一位联系人。" actions={<Button type="primary" icon={<PlusOutlined/>} onClick={create}>添加联系人</Button>}/>
  </div> : editing && draft ? <div className="detail-sheet">
    <DetailHeader
      title={isNew ? '新增联系人' : draft.name || '未命名联系人'}
      subtitle={isNew ? '填写后保存到本地工作空间' : '编辑联系人资料'}
      onBack={cancelEdit}
      backLabel={isNew ? '返回联系人列表' : '返回联系人详情'}
      actions={<div className="inline-actions"><Button onClick={cancelEdit} disabled={busy}>取消</Button><Button type="primary" loading={busy} onClick={() => void save()}>保存</Button></div>}
    />
    <div className="detail-form">
      {error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')}/>}
      <div className="form-grid">
        <label className="form-field"><span>姓名 <em>*</em></span><Input autoFocus aria-label="姓名" value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })}/></label>
        <label className="form-field"><span>性别</span><Input aria-label="性别" value={draft.gender} onChange={event => setDraft({ ...draft, gender: event.target.value })}/></label>
        <label className="form-field"><span>生日</span><Input aria-label="生日" value={draft.birthday} onChange={event => setDraft({ ...draft, birthday: event.target.value })} placeholder="YYYY-MM-DD"/></label>
        <label className="form-field"><span>电话</span><Input aria-label="电话" value={draft.phone} onChange={event => setDraft({ ...draft, phone: event.target.value })}/></label>
        <label className="form-field"><span>学校</span><Input aria-label="学校" value={draft.school} onChange={event => setDraft({ ...draft, school: event.target.value })}/></label>
        <label className="form-field"><span>专业</span><Input aria-label="专业" value={draft.major} onChange={event => setDraft({ ...draft, major: event.target.value })}/></label>
        <label className="form-field"><span>工作</span><Input aria-label="工作" value={draft.work} onChange={event => setDraft({ ...draft, work: event.target.value })}/></label>
        <label className="form-field"><span>证件号码</span><Input aria-label="证件号码" value={draft.national_id} onChange={event => setDraft({ ...draft, national_id: event.target.value })}/></label>
      </div>
      <label className="form-field"><span>地址</span><Input aria-label="地址" value={draft.address} onChange={event => setDraft({ ...draft, address: event.target.value })}/></label>
      <label className="form-field"><span>爱好</span><Input aria-label="爱好" value={draft.hobbies} onChange={event => setDraft({ ...draft, hobbies: event.target.value })}/></label>
      <label className="form-field"><span>分类</span><Select mode="multiple" aria-label="分类" style={{ width: '100%' }} value={draftTags} onChange={setDraftTags} options={data.tags.map(tag => ({ value: tag, label: tag }))} placeholder="选择已有分类"/></label>
      <label className="form-field"><span>聊天备注</span><Input.TextArea aria-label="聊天备注" rows={3} value={draft.chat_notes} onChange={event => setDraft({ ...draft, chat_notes: event.target.value })}/></label>
    </div>
  </div> : <div className="detail-sheet">
    <DetailHeader
      title={selected.name}
      subtitle={selectedTags.length ? selectedTags.join(' / ') : '未分类'}
      onBack={() => { setSelectedId(null); setDetailDismissed(true) }}
      backLabel="返回联系人列表"
      actions={<div className="inline-actions">
        <PinButton pinned={data.favorites.includes(selected.id)} label={selected.name} onClick={() => void action('contact_favorite', { id: selected.id, value: !data.favorites.includes(selected.id) })}/>
        <Button type="text" icon={<EditOutlined/>} aria-label="编辑联系人" onClick={() => startEdit(selected)}>编辑</Button>
        <Button type="text" danger icon={<DeleteOutlined/>} aria-label="删除联系人" onClick={() => void remove(selected)}>删除</Button>
      </div>}
    />
    {error && <Alert className="detail-alert" type="error" showIcon closable message={error} onClose={() => setError('')}/>}
    <div className="contact-identity">
      {photo ? <img className="contact-photo" src={photo} alt={`${selected.name}的照片`}/> : <span className="contact-photo contact-photo--empty" aria-hidden="true">{Array.from(selected.name)[0] || '?'}</span>}
      <div className="inline-actions">
        <Button onClick={() => void importAttachment('contact_import_photo')} disabled={attachmentBusy}>导入照片</Button>
        {photo && <Button onClick={() => void removeAttachment('contact_remove_photo')} disabled={attachmentBusy}>移除照片</Button>}
        <Button icon={<MessageOutlined/>} onClick={() => void importAttachment('contact_import_chats')} disabled={attachmentBusy}>导入聊天记录</Button>
      </div>
    </div>
    <div className="detail-section">
      <DetailField label="电话" value={selected.phone} actions={copyAction('电话', selected.phone)}/>
      <DetailField label="性别" value={selected.gender}/>
      <DetailField label="生日" value={selected.birthday}/>
      <DetailField label="学校" value={selected.school}/>
      <DetailField label="专业" value={selected.major}/>
      <DetailField label="工作" value={selected.work}/>
      <DetailField label="证件号码" value={selected.national_id} mono actions={copyAction('证件号码', selected.national_id)}/>
      <DetailField label="地址" value={selected.address} actions={copyAction('地址', selected.address)}/>
      <DetailField label="爱好" value={selected.hobbies}/>
      <DetailField label="分类" value={selectedTags.length ? <div className="tag-list">{selectedTags.map(tag => <span className="ant-tag" key={tag}>{tag}</span>)}</div> : ''} empty={!selectedTags.length}/>
      <DetailField label="聊天备注" value={selected.chat_notes}/>
    </div>
    <div className="detail-section">
      <div className="detail-section-title"><span>聊天记录</span><span>{selected.chats.length} 份</span></div>
      {selected.chats.length ? selected.chats.map(record => <div className="attachment-row" key={record.id}>
        <button type="button" className="attachment-open" onClick={() => setChat(record)}><MessageOutlined/><span>{record.title}</span></button>
        <div className="inline-actions">
          <Button type="text" size="small" onClick={() => void exportChat(record)}>导出原件</Button>
          <Button type="text" size="small" danger onClick={() => void removeAttachment('contact_remove_chat', record.id)} disabled={attachmentBusy}>移除</Button>
        </div>
      </div>) : <p className="meta" style={{ padding: '10px 0' }}>还没有导入聊天记录。</p>}
    </div>
  </div>

  return <div className="content collection-content" ref={layout}>
    <CollectionShell wide={wide} detailId={selected?.id} list={list} detail={detail}/>
    <Toast message={notice}/>
    <TagManagerModal open={manage} tags={data.tags} error={error} onClose={() => setManage(false)} onCreate={createTag} onRename={renameTag} onDelete={deleteTag} deleteConfirm={tag => `删除分类“${tag}”？联系人不会删除。`}/>
    <Modal title={chat?.title} width={860} open={!!chat} onCancel={() => setChat(null)} footer={<Button onClick={() => setChat(null)}>关闭</Button>}>{chat && <ChatPreview key={chat.id} chat={chat} onExport={() => void exportChat(chat)}/>}</Modal>
  </div>
}
