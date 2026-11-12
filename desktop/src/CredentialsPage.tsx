import TagManagerModal from './TagManagerModal'
import useUnsavedChanges from './useUnsavedChanges'
import CollectionShell from './ui/CollectionShell'
import { CollectionFilters, CollectionRow } from './ui/CollectionList'
import DetailHeader from './ui/DetailHeader'
import EmptyState from './ui/EmptyState'
import PinButton from './ui/PinButton'
import Toast from './ui/Toast'
import useWideLayout from './ui/useWideLayout'
import { rankBySearch } from './searchRank'
import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Input, Select } from 'antd'
import {
  CopyOutlined,
  DeleteOutlined,
  ExportOutlined,
  LockOutlined,
  PlusOutlined,
  SearchOutlined,
} from '@ant-design/icons'
import type { FocusState } from './navigation'

type Credential = { id: string; title: string; username: string; password: string; website: string; notes: string; tags: string[] }
const blank = (): Credential => ({ id: crypto.randomUUID(), title: '', username: '', password: '', website: '', notes: '', tags: [] })

export default function CredentialsPage({ setWorkspaceDirty, focus }: {
  setWorkspaceDirty?: (dirty: boolean) => void
  focus?: FocusState | null
}) {
  const [items, setItems] = useState<Credential[]>([])
  const [favorites, setFavorites] = useState<string[]>([])
  const [tags, setTags] = useState<string[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [detailDismissed, setDetailDismissed] = useState(false)
  const [draft, setDraft] = useState<Credential | null>(null)
  const [snapshot, setSnapshot] = useState('')
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [manageTags, setManageTags] = useState(false)
  const [busy, setBusy] = useState(false)
  const [loading, setLoading] = useState(true)
  const handledFocus = useRef<number | null>(null)
  const layout = useRef<HTMLDivElement>(null)
  const wide = useWideLayout(layout)

  const dirty = !!draft && JSON.stringify(draft) !== snapshot
  const isNew = !!draft && !items.some(item => item.id === draft.id)
  useUnsavedChanges(dirty, setWorkspaceDirty)
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2200); return () => window.clearTimeout(timer) }, [notice])

  const filtered = useMemo(() => {
    const byTag = items.filter(item => filter === 'all' || (filter === 'untagged' ? !item.tags.length : item.tags.includes(filter)))
    return rankBySearch(byTag, query, item => [
      { value: item.title, weight: 3 },
      { value: item.username, weight: 2 },
      { value: item.website, weight: 1.5 },
      { value: item.tags.join(' '), weight: 1.2 },
      { value: item.notes, weight: 0.6 },
    ], (a, b) => Number(favorites.includes(b.id)) - Number(favorites.includes(a.id)))
  }, [items, filter, query, favorites])

  async function refresh() {
    try {
      const [records, pinned, categories] = await Promise.all([
        invoke<Credential[]>('credentials_list'),
        invoke<string[]>('credential_favorites'),
        invoke<string[]>('credential_tags'),
      ])
      setItems(records)
      setFavorites(pinned)
      setTags(categories)
      setError('')
    } catch (e) {
      setError(`读取账号失败：${String(e)}`)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    let cancelled = false
    void Promise.all([
      invoke<Credential[]>('credentials_list'),
      invoke<string[]>('credential_favorites'),
      invoke<string[]>('credential_tags'),
    ]).then(([records, pinned, categories]) => {
      if (cancelled) return
      setItems(records)
      setFavorites(pinned)
      setTags(categories)
      setError('')
    }).catch(e => { if (!cancelled) setError(`读取账号失败：${String(e)}`) })
      .finally(() => { if (!cancelled) setLoading(false) })
    return () => { cancelled = true }
  }, [])

  // 详情始终是可编辑表单：选中记录后立即生成草稿，不再需要“编辑”按钮。
  useEffect(() => {
    if (!selectedId) {
      setDraft(null)
      setSnapshot('')
      return
    }
    if (draft?.id === selectedId) return
    const item = items.find(record => record.id === selectedId)
    if (!item) return
    setDraft({ ...item, tags: [...item.tags] })
    setSnapshot(JSON.stringify(item))
  }, [selectedId, items, draft?.id])

  useEffect(() => {
    if (!wide || detailDismissed) return
    // 保留正在新增、尚未入库的草稿，避免被自动选中逻辑覆盖。
    if (draft && !items.some(item => item.id === draft.id)) return
    if (selectedId && (dirty || filtered.some(item => item.id === selectedId))) return
    setSelectedId(filtered[0]?.id ?? null)
  }, [filtered, items, selectedId, detailDismissed, wide, draft, dirty])

  useEffect(() => {
    if (!focus?.id || handledFocus.current === focus.nonce) return
    const item = items.find(record => record.id === focus.id)
    if (item) {
      handledFocus.current = focus.nonce
      setQuery('')
      setFilter('all')
      setSelectedId(item.id)
      setDraft({ ...item, tags: [...item.tags] })
      setSnapshot(JSON.stringify(item))
      setDetailDismissed(false)
      setError('')
    }
  }, [focus?.nonce, focus?.id, items])

  useEffect(() => { setDetailDismissed(false) }, [query, filter])

  function select(item: Credential) {
    if (item.id === selectedId) {
      setDetailDismissed(false)
      return
    }
    if (dirty && !window.confirm('当前账号有未保存的修改，确定切换吗？')) return
    setSelectedId(item.id)
    setDetailDismissed(false)
    setError('')
  }

  function create() {
    if (dirty && !window.confirm('当前账号有未保存的修改，确定新增吗？')) return
    const item = blank()
    setDraft(item)
    setSnapshot(JSON.stringify(item))
    setSelectedId(item.id)
    setDetailDismissed(false)
    setError('')
  }

  function discard() {
    if (!draft) return
    if (dirty && !window.confirm('确定放弃未保存的修改吗？')) return
    const item = items.find(record => record.id === draft.id)
    if (item) {
      setDraft({ ...item, tags: [...item.tags] })
      setSnapshot(JSON.stringify(item))
      setError('')
      return
    }
    setDraft(null)
    setSnapshot('')
    setSelectedId(null)
    setDetailDismissed(true)
    setError('')
  }

  function closeDetail() {
    if (dirty && !window.confirm('账号信息有未保存的修改，确定放弃吗？')) return
    setDraft(null)
    setSnapshot('')
    setSelectedId(null)
    setDetailDismissed(true)
    setError('')
  }

  async function save() {
    if (!draft || busy) return
    if (!draft.title.trim()) { setError('请填写名称'); return }
    setBusy(true)
    try {
      const entry = { ...draft, title: draft.title.trim() }
      await invoke('credentials_save', { entry })
      await refresh()
      setDraft(entry)
      setSnapshot(JSON.stringify(entry))
      setSelectedId(entry.id)
      setNotice('账号已保存')
      setError('')
    } catch (e) {
      setError(`保存失败：${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  async function remove(item: Credential) {
    if (!window.confirm(`确定删除“${item.title}”吗？`)) return
    try {
      await invoke('credentials_delete', { id: item.id })
      if (selectedId === item.id) {
        setDraft(null)
        setSnapshot('')
        setSelectedId(null)
      }
      await refresh()
      setNotice('账号已删除')
    } catch (e) {
      setError(`删除失败：${String(e)}`)
    }
  }

  async function copy(label: string, value: string) {
    if (!value) return
    try {
      await invoke('copy_value', { value })
      setNotice(`${label}已复制`)
      setError('')
    } catch {
      setError('复制失败，请检查剪贴板权限')
    }
  }

  async function openWebsite() {
    const value = draft?.website.trim()
    if (!value) return
    const url = /^[a-z][a-z0-9+.-]*:\/\//i.test(value) ? value : `https://${value}`
    try {
      await invoke('websites_open', { url })
      setError('')
    } catch (e) {
      setError(`打开网址失败：${String(e)}`)
    }
  }

  async function toggleFavorite(item: Credential) {
    try {
      await invoke('credential_favorite', { id: item.id, value: !favorites.includes(item.id) })
      await refresh()
    } catch (e) {
      setError(`操作失败：${String(e)}`)
    }
  }

  async function generatePassword() {
    if (!draft) return
    try {
      const password = await invoke<string>('password_generate')
      setDraft({ ...draft, password })
      setNotice('已生成随机密码')
      setError('')
    } catch (e) {
      setError(`生成密码失败：${String(e)}`)
    }
  }

  async function createTag(name: string) {
    try { await invoke('credential_tag_create', { name }); await refresh(); return true } catch (e) { setError(`添加分类失败：${String(e)}`); return false }
  }
  async function renameTag(original: string, name: string) {
    try { await invoke('credential_tag_rename', { original, name }); if (filter === original) setFilter(name); await refresh(); return true } catch (e) { setError(`修改分类失败：${String(e)}`); return false }
  }
  async function deleteTag(name: string) {
    try { await invoke('credential_tag_delete', { name }); if (filter === name) setFilter('all'); await refresh(); return true } catch (e) { setError(`删除分类失败：${String(e)}`); return false }
  }

  const list = <>
    <div className="collection-list-head">
      <div className="collection-list-heading"><h1>账号密码</h1>
        <span className="collection-count">{loading ? '读取中' : `${filtered.length} 条`}</span>
      </div>
      <div className="collection-list-actions">
        <Button type="primary" icon={<PlusOutlined/>} onClick={create}>新增账号</Button>
        <Button type="text" onClick={() => { setError(''); setManageTags(true) }}>管理分类</Button>
      </div>
    </div>
    {!draft && error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')} style={{ margin: '0 14px 10px' }}/>}
    <div className="collection-search">
      <Input aria-label="搜索账号" prefix={<SearchOutlined/>} placeholder="搜索名称、账号、网址或备注" value={query} onChange={event => setQuery(event.target.value)} allowClear/>
    </div>
    <CollectionFilters tags={tags} selected={filter} onChange={setFilter}/>
    <div className="collection-list" role="list" aria-label="账号记录" aria-busy={loading}>
      {isNew && draft && <CollectionRow id={draft.id} title={draft.title || '新账号'} subtitle="正在添加" current onSelect={() => setSelectedId(draft.id)}/>}
      {filtered.map(item => <CollectionRow
        key={item.id}
        id={item.id}
        title={item.title}
        subtitle={item.username || item.website || '未填写账号'}
        current={!isNew && selectedId === item.id}
        pinned={favorites.includes(item.id)}
        onSelect={() => select(item)}
      />)}
      {!filtered.length && !isNew && <div className="collection-list-empty" role="status">
        <span>{loading ? '正在读取账号…' : error && !items.length ? '暂时无法读取账号。' : query || filter !== 'all' ? '没有找到匹配的账号。' : '还没有账号记录。'}</span>
        {!loading && (query || filter !== 'all') && <Button type="link" size="small" onClick={() => { setQuery(''); setFilter('all') }}>清除筛选</Button>}
        {!loading && error && !items.length && <Button size="small" onClick={() => { setLoading(true); void refresh() }}>重试</Button>}
      </div>}
    </div>
  </>

  const detail = !draft ? <div className="detail-empty">
    <EmptyState icon={<LockOutlined/>} title="从左侧选择一条账号" description="账号、密码和备注会在这里展开；也可以新增一条记录。" actions={<Button type="primary" icon={<PlusOutlined/>} onClick={create}>新增账号</Button>}/>
  </div> : <div className="detail-sheet">
    <DetailHeader
      title={isNew ? '新增账号' : draft.title || '未命名账号'}
      subtitle={isNew ? '填写后保存到本地工作空间' : dirty ? '有未保存的修改' : '直接修改字段，保存后生效'}
      onBack={closeDetail}
      backLabel="返回账号列表"
      actions={<div className="inline-actions">
        {!isNew && <PinButton pinned={favorites.includes(draft.id)} label={draft.title} onClick={() => void toggleFavorite(draft)}/>}
        {!isNew && <Button type="text" danger icon={<DeleteOutlined/>} aria-label="删除账号" onClick={() => void remove(draft)}>删除</Button>}
        <Button onClick={discard} disabled={busy || (!dirty && !isNew)}>{isNew ? '取消' : '还原'}</Button>
        <Button type="primary" loading={busy} onClick={() => void save()}>保存账号</Button>
      </div>}
    />
    <div className="detail-form">
      {error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')}/>}
      <div className="form-field"><span>名称 <em>*</em></span><Input autoFocus={isNew} aria-label="名称" value={draft.title} onChange={event => setDraft({ ...draft, title: event.target.value })} placeholder="例如：工作邮箱"/></div>
      <div className="form-field"><span>账号</span><div style={{ display: 'flex', gap: 8 }}><Input style={{ flex: 1 }} aria-label="账号" value={draft.username} onChange={event => setDraft({ ...draft, username: event.target.value })} placeholder="用户名或邮箱"/><Button type="text" icon={<CopyOutlined/>} aria-label="复制账号" disabled={!draft.username} onClick={() => void copy('账号', draft.username)}/></div></div>
      <div className="form-field"><span>密码</span><div style={{ display: 'flex', gap: 8 }}><Input.Password style={{ flex: 1 }} aria-label="密码" value={draft.password} onChange={event => setDraft({ ...draft, password: event.target.value })} placeholder="输入密码"/><Button onClick={() => void generatePassword()}>生成</Button><Button type="text" icon={<CopyOutlined/>} aria-label="复制密码" disabled={!draft.password} onClick={() => void copy('密码', draft.password)}/></div></div>
      <div className="form-field"><span>网址或应用</span><div style={{ display: 'flex', gap: 8 }}><Input style={{ flex: 1 }} aria-label="网址或应用" value={draft.website} onChange={event => setDraft({ ...draft, website: event.target.value })} placeholder="可选"/><Button type="text" icon={<ExportOutlined/>} aria-label="打开网址" title="在浏览器中打开" disabled={!draft.website.trim()} onClick={() => void openWebsite()}/><Button type="text" icon={<CopyOutlined/>} aria-label="复制网址或应用" disabled={!draft.website} onClick={() => void copy('网址', draft.website)}/></div></div>
      <div className="form-field"><span>分类</span><Select mode="multiple" aria-label="分类" style={{ width: '100%' }} value={draft.tags} onChange={value => setDraft({ ...draft, tags: value })} options={tags.map(tag => ({ value: tag, label: tag }))} placeholder="选择已有分类"/></div>
      <div className="form-field"><span>备注</span><div style={{ display: 'flex', gap: 8, alignItems: 'flex-start' }}><Input.TextArea style={{ flex: 1 }} aria-label="备注" rows={4} value={draft.notes} onChange={event => setDraft({ ...draft, notes: event.target.value })} placeholder="添加需要记住的其他信息"/><Button type="text" icon={<CopyOutlined/>} aria-label="复制备注" disabled={!draft.notes} onClick={() => void copy('备注', draft.notes)}/></div></div>
    </div>
  </div>

  return <div className="content collection-content" ref={layout}>
    <CollectionShell wide={wide} detailId={draft?.id} list={list} detail={detail}/>
    <Toast message={notice}/>
    <TagManagerModal open={manageTags} tags={tags} error={error} onClose={() => setManageTags(false)} onCreate={createTag} onRename={renameTag} onDelete={deleteTag} deleteConfirm={name => `确定删除分类“${name}”？账号本身不会删除。`}/>
  </div>
}
