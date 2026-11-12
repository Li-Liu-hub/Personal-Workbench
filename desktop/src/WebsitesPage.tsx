import TagManagerModal from './TagManagerModal'
import useUnsavedChanges from './useUnsavedChanges'
import EmptyState from './ui/EmptyState'
import PinButton from './ui/PinButton'
import Toast from './ui/Toast'
import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Drawer, Input, Popconfirm, Select, Space } from 'antd'
import { CopyOutlined, GlobalOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import type { FocusState } from './navigation'

type Website = { id: string; title: string; url: string; category: string; notes: string }
type Data = { tags: string[]; assignments: { id: string; tags: string[] }[]; favorites: string[]; view: 'grid' | 'list' }

export default function WebsitesPage({ setWorkspaceDirty, focus }: {
  setWorkspaceDirty?: (dirty: boolean) => void
  focus?: FocusState | null
}) {
  const [items, setItems] = useState<Website[]>([])
  const [data, setData] = useState<Data>({ tags: [], assignments: [], favorites: [], view: 'grid' })
  const [draft, setDraft] = useState<Website | null>(null)
  const [draftTags, setDraftTags] = useState<string[]>([])
  const [snapshot, setSnapshot] = useState('')
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [manage, setManage] = useState(false)
  const [busy, setBusy] = useState(false)
  const handledFocus = useRef<number | null>(null)
  const dirty = !!draft && JSON.stringify([draft, draftTags]) !== snapshot
  useUnsavedChanges(dirty, setWorkspaceDirty)
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2400); return () => window.clearTimeout(timer) }, [notice])

  const labels = (id: string) => data.assignments.find(item => item.id === id)?.tags || []

  async function refresh() {
    try {
      const [list, extra] = await Promise.all([
        invoke<Website[]>('websites_list'),
        invoke<{ tags: string[]; assignments: { id: string; tags: string[] }[]; favorites: string[]; view: 'grid' | 'list' }>('website_data'),
      ])
      setItems(list)
      setData({ tags: extra.tags, assignments: extra.assignments, favorites: extra.favorites, view: extra.view || 'grid' })
      setError('')
    } catch (e) {
      setError(`读取网站失败：${String(e)}`)
    }
  }

  useEffect(() => { void refresh() }, [])

  useEffect(() => {
    if (!focus?.id || handledFocus.current === focus.nonce) return
    const item = items.find(record => record.id === focus.id)
    if (item) {
      handledFocus.current = focus.nonce
      setDraft({ ...item })
      setDraftTags([...labels(item.id)])
      setSnapshot(JSON.stringify([item, labels(item.id)]))
      setError('')
    }
  }, [focus?.nonce, focus?.id, items])

  const visible = items.filter(item => {
    const itemTags = labels(item.id)
    return (filter === 'all' || (filter === 'untagged' ? itemTags.length === 0 : itemTags.includes(filter)))
      && [item.title, item.url, item.category, item.notes, ...itemTags].some(value => value.toLowerCase().includes(query.trim().toLowerCase()))
  }).sort((a, b) => Number(data.favorites.includes(b.id)) - Number(data.favorites.includes(a.id)))

  function edit(item: Website) {
    const tags = labels(item.id)
    setDraft({ ...item })
    setDraftTags([...tags])
    setSnapshot(JSON.stringify([item, tags]))
    setError('')
  }

  function close() {
    if (draft && JSON.stringify([draft, draftTags]) !== snapshot && !window.confirm('网站信息有未保存的修改，确定关闭吗？')) return
    setDraft(null)
  }

  async function save() {
    if (!draft || busy) return
    if (!draft.title.trim()) { setError('请填写名称'); return }
    setBusy(true)
    try {
      await invoke('website_save_classified', { entry: { ...draft, title: draft.title.trim() }, tags: draftTags })
      setDraft(null)
      await refresh()
      setNotice('网站已保存')
    } catch (e) {
      setError(`保存网站失败：${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  async function openSite(url: string) {
    try { await invoke('websites_open', { url }) } catch (e) { setError(String(e)) }
  }

  async function copyUrl(url: string) {
    try { await invoke('copy_value', { value: url }); setNotice('网址已复制') } catch (e) { setError(`复制失败：${String(e)}`) }
  }

  async function remove(id: string) {
    try { await invoke('websites_delete', { id }); await refresh(); setNotice('网站已删除') } catch (e) { setError(`删除网站失败：${String(e)}`) }
  }

  async function action(command: string, args: Record<string, unknown>) {
    try { await invoke(command, args); await refresh(); return true } catch (e) { setError(`操作失败：${String(e)}`); return false }
  }
  async function createTag(name: string) { return action('website_tag_create', { name }) }
  async function renameTag(original: string, name: string) {
    const ok = await action('website_tag_rename', { original, name })
    if (ok && filter === original) setFilter(name)
    return ok
  }
  async function deleteTag(name: string) {
    const ok = await action('website_tag_delete', { name })
    if (ok && filter === name) setFilter('all')
    return ok
  }

  return <div className="content">
    <header className="page-header">
      <div><h1>网站收藏</h1><p>整理常用网站，一键打开或复制网址</p></div>
      <Space><Button type="primary" icon={<PlusOutlined/>} onClick={() => edit({ id: crypto.randomUUID(), title: '', url: '', category: '', notes: '' })}>添加网站</Button></Space>
    </header>
    {error && <Alert type="error" message={error} showIcon closable onClose={() => setError('')} style={{ marginBottom: 16 }}/>}
    <section className="toolbar">
      <Input aria-label="搜索网站" prefix={<SearchOutlined/>} placeholder="搜索名称、网址或备注" value={query} onChange={event => setQuery(event.target.value)} allowClear/>
      <div className="toolbar-controls">
        <Select aria-label="分类筛选" value={filter} onChange={setFilter} style={{ width: 150 }} options={[{ value: 'all', label: '全部分类' }, { value: 'untagged', label: '未分类' }, ...data.tags.map(tag => ({ value: tag, label: tag }))]}/>
        <Button type="text" onClick={() => { setError(''); setManage(true) }}>管理分类</Button>
      </div>
    </section>
    <div className="section-heading"><div><h2>网站列表</h2><span>共 {visible.length} 个</span></div></div>
    {visible.length ? <section className="bookmark-grid" aria-label="网站列表">
      {visible.map(item => <article className={`bookmark-card ${data.favorites.includes(item.id) ? 'pinned' : ''}`} key={item.id}>
        <button className="bookmark-launch" onClick={() => edit(item)} aria-label={`编辑${item.title}`}>
          <span className="bookmark-mark" aria-hidden="true">{Array.from(item.title || '?')[0]}</span>
          <span className="bookmark-copy"><strong>{item.title}</strong><small>{item.url}</small></span>
        </button>
        {labels(item.id).length > 0 && <div className="tag-list">{labels(item.id).map(tag => <span className="ant-tag" key={tag}>{tag}</span>)}</div>}
        <div className="bookmark-actions">
          <PinButton pinned={data.favorites.includes(item.id)} label={item.title} onClick={() => void action('website_favorite', { id: item.id, value: !data.favorites.includes(item.id) })}/>
          <Button size="small" onClick={() => void openSite(item.url)}>打开</Button>
          <Button size="small" type="text" icon={<CopyOutlined/>} aria-label={`复制${item.title}网址`} onClick={() => void copyUrl(item.url)}/>
          <Button size="small" type="text" onClick={() => edit(item)}>编辑</Button>
          <Popconfirm title={`删除“${item.title}”？`} okText="删除" cancelText="取消" onConfirm={() => void remove(item.id)}><Button size="small" type="text" danger>删除</Button></Popconfirm>
        </div>
      </article>)}
    </section> : <EmptyState icon={<GlobalOutlined/>} title={query || filter !== 'all' ? '没有找到匹配的网站' : '还没有网站收藏'} description={query || filter !== 'all' ? '试试其他关键词或分类。' : '添加第一个网站，把常用入口放在手边。'} actions={!query && filter === 'all' ? <Button type="primary" icon={<PlusOutlined/>} onClick={() => edit({ id: crypto.randomUUID(), title: '', url: '', category: '', notes: '' })}>添加网站</Button> : undefined}/>}
    <TagManagerModal open={manage} tags={data.tags} error={error} onClose={() => setManage(false)} onCreate={createTag} onRename={renameTag} onDelete={deleteTag} deleteConfirm={tag => `删除分类“${tag}”？网站不会删除。`}/>
    <Drawer title="网站详情" open={!!draft} onClose={close} width={480} destroyOnHidden footer={<Space style={{ float: 'right' }}><Button onClick={close}>取消</Button><Button type="primary" loading={busy} onClick={() => void save()}>保存</Button></Space>}>
      {draft && <div className="detail-form">
        {error && <Alert type="error" showIcon message={error}/>}
        <label className="form-field"><span>名称 <em>*</em></span><Input autoFocus aria-label="网站名称" value={draft.title} onChange={event => setDraft({ ...draft, title: event.target.value })}/></label>
        <label className="form-field"><span>网址</span><Input aria-label="网站网址" value={draft.url} onChange={event => setDraft({ ...draft, url: event.target.value })}/></label>
        <label className="form-field"><span>分类</span><Select mode="multiple" aria-label="分类" style={{ width: '100%' }} value={draftTags} onChange={setDraftTags} options={data.tags.map(tag => ({ value: tag, label: tag }))}/></label>
        <label className="form-field"><span>备注</span><Input.TextArea aria-label="网站备注" rows={4} value={draft.notes} onChange={event => setDraft({ ...draft, notes: event.target.value })}/></label>
      </div>}
    </Drawer>
    <Toast message={notice}/>
  </div>
}