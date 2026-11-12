import TagManagerModal from './TagManagerModal'
import useUnsavedChanges from './useUnsavedChanges'
import EmptyState from './ui/EmptyState'
import PinButton from './ui/PinButton'
import Toast from './ui/Toast'
import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Drawer, Input, Popconfirm, Select, Space } from 'antd'
import { FolderOpenOutlined, PlusOutlined, SearchOutlined, ToolOutlined } from '@ant-design/icons'
import type { FocusState } from './navigation'

type Tool = { id: string; name: string; path: string; notes: string }
type ToolData = { items: Tool[]; favorites: string[]; tags: string[]; assignments: { id: string; tags: string[] }[]; view: 'grid' | 'list' }
const initial: ToolData = { items: [], favorites: [], tags: [], assignments: [], view: 'grid' }

export default function ToolsPage({ setWorkspaceDirty, focus }: {
  setWorkspaceDirty?: (dirty: boolean) => void
  focus?: FocusState | null
}) {
  const [data, setData] = useState<ToolData>(initial)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [draft, setDraft] = useState<Tool | null>(null)
  const [draftTags, setDraftTags] = useState<string[]>([])
  const [snapshot, setSnapshot] = useState('')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const [manageTags, setManageTags] = useState(false)
  const [highlightId, setHighlightId] = useState<string | null>(null)
  const handledFocus = useRef<number | null>(null)
  const lastLaunch = useRef<{ path: string; at: number } | null>(null)
  const dirty = !!draft && JSON.stringify([draft, draftTags]) !== snapshot
  useUnsavedChanges(dirty, setWorkspaceDirty)
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2400); return () => window.clearTimeout(timer) }, [notice])

  function labels(id: string) { return data.assignments.find(item => item.id === id)?.tags || [] }

  async function reload() {
    try { setData(await invoke<ToolData>('tool_data')); setError('') } catch (e) { setError(`读取小工具失败：${String(e)}`) }
  }

  useEffect(() => { void reload() }, [])

  useEffect(() => {
    if (!focus?.id || handledFocus.current === focus.nonce) return
    if (data.items.some(item => item.id === focus.id)) {
      handledFocus.current = focus.nonce
      setHighlightId(focus.id)
    }
  }, [focus?.nonce, focus?.id, data.items])

  const visible = useMemo(() => data.items.filter(tool => {
    const toolTags = data.assignments.find(item => item.id === tool.id)?.tags || []
    return (filter === 'all' || (filter === 'untagged' ? !toolTags.length : toolTags.includes(filter)))
      && [tool.name, tool.path, tool.notes, ...toolTags].some(value => value.toLowerCase().includes(query.trim().toLowerCase()))
  }).sort((a, b) => Number(data.favorites.includes(b.id)) - Number(data.favorites.includes(a.id))), [data, query, filter])

  function close() {
    if (busy) return
    if (dirty && !window.confirm('程序入口有未保存的修改，确定关闭吗？')) return
    setDraft(null)
  }

  async function pick() {
    try {
      const path = await invoke<string | null>('tool_pick')
      if (!path) return
      if (draft) setDraft({ ...draft, path })
      else {
        const filename = path.split(/[\\/]/).pop() || ''
        const tool = { id: crypto.randomUUID(), name: filename.replace(/\.exe$/i, ''), path, notes: '' }
        setSnapshot(JSON.stringify([tool, []]))
        setDraft(tool)
        setDraftTags([])
      }
    } catch (e) {
      setError(`选择程序失败：${String(e)}`)
    }
  }

  async function save() {
    if (!draft || busy) return
    if (!draft.name.trim()) { setError('请填写名称'); return }
    setBusy(true)
    try {
      await invoke('tools_save_classified', { entry: { ...draft, name: draft.name.trim() }, tags: draftTags })
      setDraft(null)
      await reload()
      setNotice('程序入口已保存')
    } catch (e) {
      setError(`保存入口失败：${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  async function remove(id: string) {
    try { await invoke('tools_delete', { id }); await reload(); setNotice('程序入口已移除') } catch (e) { setError(`移除入口失败：${String(e)}`) }
  }

  async function pin(id: string) {
    try { await invoke('tool_favorite', { id, value: !data.favorites.includes(id) }); await reload() } catch (e) { setError(`设置置顶失败：${String(e)}`) }
  }

  async function launch(tool: Tool) {
    const now = Date.now()
    if (lastLaunch.current?.path === tool.path && now - lastLaunch.current.at < 650) return
    lastLaunch.current = { path: tool.path, at: now }
    try { await invoke('tools_launch', { path: tool.path }); setNotice(`已启动「${tool.name}」`) } catch (e) { setError(`启动失败：${String(e)}`) }
  }

  async function addTag(name: string) {
    try { await invoke('tool_tag_create', { name }); await reload(); return true } catch (e) { setError(`添加分类失败：${String(e)}`); return false }
  }
  async function renameTag(original: string, name: string) {
    try { await invoke('tool_tag_rename', { original, name }); if (filter === original) setFilter(name); await reload(); return true } catch (e) { setError(`修改分类失败：${String(e)}`); return false }
  }
  async function deleteTag(name: string) {
    try { await invoke('tool_tag_delete', { name }); if (filter === name) setFilter('all'); await reload(); return true } catch (e) { setError(`删除分类失败：${String(e)}`); return false }
  }

  function edit(tool: Tool) {
    const tags = labels(tool.id)
    setDraft({ ...tool })
    setDraftTags([...tags])
    setSnapshot(JSON.stringify([tool, tags]))
    setError('')
  }

  return <div className="content">
    <header className="page-header">
      <div><h1>小工具</h1><p>单击启动程序；移除入口不会删除原程序</p></div>
      <Space><Button type="primary" icon={<PlusOutlined/>} onClick={() => void pick()}>添加程序</Button></Space>
    </header>
    {error && <Alert type="error" showIcon closable onClose={() => setError('')} message={error} style={{ marginBottom: 16 }}/>}
    <section className="toolbar">
      <Input className="search-antd" prefix={<SearchOutlined/>} allowClear aria-label="搜索小工具" placeholder="搜索名称、路径或备注" value={query} onChange={event => setQuery(event.target.value)}/>
      <div className="toolbar-controls">
        <Select aria-label="按分类筛选" style={{ width: 140 }} value={filter} onChange={setFilter} options={[{ value: 'all', label: '全部分类' }, { value: 'untagged', label: '未分类' }, ...data.tags.map(tag => ({ value: tag, label: tag }))]}/>
        <Button type="text" onClick={() => { setError(''); setManageTags(true) }}>管理分类</Button>
      </div>
    </section>
    <div className="section-heading"><div><h2>程序入口</h2><span>共 {visible.length} 个</span></div></div>
    {visible.length ? <section className="tool-tile-grid" aria-label="程序入口">
      {visible.map(tool => <article className={`tool-tile ${data.favorites.includes(tool.id) ? 'pinned' : ''} ${highlightId === tool.id ? 'highlighted' : ''}`} key={tool.id}>
        <button className="tool-tile-launch" onClick={() => void launch(tool)} title="启动程序">
          <span className="tool-tile-icon"><ToolOutlined/></span>
          <span className="tool-tile-copy"><strong>{tool.name}</strong><small title={tool.path}>{tool.path}</small></span>
        </button>
        {labels(tool.id).length > 0 && <div className="tag-list">{labels(tool.id).map(tag => <span className="ant-tag" key={tag}>{tag}</span>)}</div>}
        <div className="tool-tile-actions">
          <PinButton pinned={data.favorites.includes(tool.id)} label={tool.name} onClick={() => void pin(tool.id)}/>
          <Button type="text" size="small" onClick={() => edit(tool)}>编辑</Button>
          <Popconfirm title="移除程序入口？" description="原程序不会被删除。" okText="移除" cancelText="取消" onConfirm={() => void remove(tool.id)}><Button type="text" size="small" danger>移除</Button></Popconfirm>
        </div>
      </article>)}
    </section> : <EmptyState icon={<ToolOutlined/>} title={query || filter !== 'all' ? '没有找到匹配的程序' : '还没有程序入口'} description={query || filter !== 'all' ? '试试其他关键词或分类。' : '添加一个程序，之后从桌面一键启动。'} actions={!query && filter === 'all' ? <Button type="primary" icon={<PlusOutlined/>} onClick={() => void pick()}>添加程序</Button> : undefined}/>}
    <TagManagerModal open={manageTags} tags={data.tags} error={error} onClose={() => setManageTags(false)} onCreate={addTag} onRename={renameTag} onDelete={deleteTag} deleteConfirm={name => `确定删除分类“${name}”？程序入口不会被删除。`}/>
    <Drawer title="快捷程序" open={!!draft} onClose={close} width={480} destroyOnHidden footer={<Space style={{ float: 'right' }}><Button onClick={close}>取消</Button><Button type="primary" loading={busy} onClick={() => void save()}>保存入口</Button></Space>}>
      {draft && <div className="detail-form">
        {error && <Alert type="error" showIcon message={error}/>}
        <p className="meta">移除快捷入口不会删除目标程序。</p>
        <label className="form-field"><span>名称 <em>*</em></span><Input aria-label="名称" value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })}/></label>
        <label className="form-field"><span>程序路径</span><Space.Compact style={{ width: '100%' }}><Input aria-label="程序路径" value={draft.path} onChange={event => setDraft({ ...draft, path: event.target.value })}/><Button icon={<FolderOpenOutlined/>} onClick={() => void pick()}>选择</Button></Space.Compact></label>
        <label className="form-field"><span>分类</span><Select mode="multiple" aria-label="分类" style={{ width: '100%' }} value={draftTags} onChange={setDraftTags} options={data.tags.map(tag => ({ value: tag, label: tag }))} placeholder="选择已有分类"/></label>
        <label className="form-field"><span>备注</span><Input.TextArea aria-label="备注" rows={3} value={draft.notes} onChange={event => setDraft({ ...draft, notes: event.target.value })}/></label>
      </div>}
    </Drawer>
    <Toast message={notice}/>
  </div>
}