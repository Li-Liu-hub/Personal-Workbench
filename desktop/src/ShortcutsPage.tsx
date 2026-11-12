import TagManagerModal from './TagManagerModal'
import useUnsavedChanges from './useUnsavedChanges'
import EmptyState from './ui/EmptyState'
import PinButton from './ui/PinButton'
import Toast from './ui/Toast'
import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Drawer, Input, Popconfirm, Select, Space } from 'antd'
import { FileOutlined, FolderOpenOutlined, FolderOutlined, PlusOutlined, SearchOutlined, UndoOutlined } from '@ant-design/icons'
import type { FocusState } from './navigation'

type Item = { id: string; name: string; path: string; is_dir: boolean; pinned: boolean }
type Data = { items: Item[]; tags: string[]; assignments: { id: string; tags: string[] }[]; favorites: string[]; view: 'grid' | 'list'; undo: boolean }
const empty: Data = { items: [], tags: [], assignments: [], favorites: [], view: 'list', undo: false }

export default function ShortcutsPage({ setWorkspaceDirty, focus }: {
  setWorkspaceDirty?: (dirty: boolean) => void
  focus?: FocusState | null
}) {
  const [data, setData] = useState<Data>(empty)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [draft, setDraft] = useState<{ name: string; path: string; is_dir: boolean } | null>(null)
  const [manage, setManage] = useState(false)
  const [busy, setBusy] = useState(false)
  const [highlightId, setHighlightId] = useState<string | null>(null)
  const handledFocus = useRef<number | null>(null)
  useUnsavedChanges(!!draft, setWorkspaceDirty)
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2400); return () => window.clearTimeout(timer) }, [notice])

  const labels = (id: string) => data.assignments.find(item => item.id === id)?.tags || []

  async function refresh() {
    try { setData(await invoke<Data>('shortcut_data')); setError('') } catch (e) { setError(`读取快捷路径失败：${String(e)}`) }
  }

  useEffect(() => { void refresh() }, [])

  useEffect(() => {
    if (!focus?.id || handledFocus.current === focus.nonce) return
    if (data.items.some(item => item.id === focus.id)) {
      handledFocus.current = focus.nonce
      setHighlightId(focus.id)
    }
  }, [focus?.nonce, focus?.id, data.items])

  const visible = useMemo(() => data.items.filter(item => {
    const itemTags = labels(item.id)
    return (filter === 'all' || (filter === 'untagged' ? !itemTags.length : itemTags.includes(filter)))
      && [item.name, item.path, ...itemTags].some(value => value.toLowerCase().includes(query.trim().toLowerCase()))
  }).sort((a, b) => Number(data.favorites.includes(b.id)) - Number(data.favorites.includes(a.id))), [data, query, filter])

  async function action(command: string, args: Record<string, unknown> = {}) {
    try { await invoke(command, args); await refresh(); return true } catch (e) { setError(`操作失败：${String(e)}`); return false }
  }
  async function createTag(name: string) { return action('shortcut_tag_create', { name }) }
  async function renameTag(original: string, name: string) {
    const ok = await action('shortcut_tag_rename', { original, name })
    if (ok && filter === original) setFilter(name)
    return ok
  }
  async function deleteTag(name: string) {
    const ok = await action('shortcut_tag_delete', { name })
    if (ok && filter === name) setFilter('all')
    return ok
  }

  async function pick(is_dir: boolean) {
    try {
      const path = await invoke<string | null>('shortcut_pick', { isDir: is_dir })
      if (path) setDraft({ name: path.split(/[\\/]/).filter(Boolean).pop() || path, path, is_dir })
    } catch (e) {
      setError(`选择路径失败：${String(e)}`)
    }
  }

  function close() {
    if (draft && !window.confirm('快捷路径尚未添加，确定关闭吗？')) return
    setDraft(null)
  }

  async function add() {
    if (!draft || busy) return
    setBusy(true)
    try {
      await invoke('shortcut_add', { name: draft.name, path: draft.path, isDir: draft.is_dir })
      setDraft(null)
      await refresh()
      setNotice('快捷路径已添加')
    } catch (e) {
      setError(`添加失败：${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  return <div className="content">
    <header className="page-header">
      <div><h1>快捷路径</h1><p>快速打开文件或文件夹所在位置</p></div>
      <Space><Button icon={<FileOutlined/>} onClick={() => void pick(false)}>添加文件</Button><Button type="primary" icon={<PlusOutlined/>} onClick={() => void pick(true)}>添加文件夹</Button></Space>
    </header>
    {error && <Alert type="error" showIcon message={error} closable onClose={() => setError('')} style={{ marginBottom: 16 }}/>}
    <section className="toolbar">
      <Input aria-label="搜索快捷路径" prefix={<SearchOutlined/>} placeholder="搜索名称或路径" value={query} onChange={event => setQuery(event.target.value)} allowClear/>
      <div className="toolbar-controls">
        <Select aria-label="分类筛选" value={filter} onChange={setFilter} style={{ width: 140 }} options={[{ value: 'all', label: '全部分类' }, { value: 'untagged', label: '未分类' }, ...data.tags.map(tag => ({ value: tag, label: tag }))]}/>
        <Button type="text" onClick={() => { setError(''); setManage(true) }}>管理分类</Button>
        <Button type="text" icon={<UndoOutlined/>} disabled={!data.undo} onClick={() => void action('shortcut_undo')}>撤销移除</Button>
      </div>
    </section>
    <div className="section-heading"><div><h2>路径索引</h2><span>共 {visible.length} 条</span></div></div>
    {visible.length ? <section className="shortcut-list" aria-label="快捷路径列表">
      {visible.map(item => <article className={`shortcut-row ${highlightId === item.id ? 'highlighted' : ''}`} key={item.id}>
        <button type="button" className="shortcut-open" title={`${item.name}\n${item.path}`} onClick={() => void action('shortcut_open', { path: item.path })}>
          <span className="shortcut-icon">{item.is_dir ? <FolderOutlined/> : <FileOutlined/>}</span>
          <span className="shortcut-copy"><strong>{item.name || item.path.split(/[\\/]/).filter(Boolean).pop() || item.path}</strong><small>{item.path}</small></span>
        </button>
        <div className="tag-list">{labels(item.id).map(tag => <span className="ant-tag" key={tag}>{tag}</span>)}</div>
        <div className="shortcut-row-actions">
          <Select className="shortcut-category" mode="multiple" aria-label={`${item.name}分类`} placeholder="分类" value={labels(item.id)} onChange={tags => void action('shortcut_tags', { id: item.id, tags })} options={data.tags.map(tag => ({ value: tag, label: tag }))}/>
          <PinButton pinned={data.favorites.includes(item.id)} label={item.name} onClick={() => void action('shortcut_favorite', { id: item.id, value: !data.favorites.includes(item.id) })}/>
          <Popconfirm title="移除快捷索引？" description="不会删除原文件或文件夹。" okText="移除" cancelText="取消" onConfirm={() => void action('shortcut_remove', { id: item.id }).then(ok => { if (ok) setNotice('已移除索引，原文件不受影响') })}><Button type="text" danger>移除</Button></Popconfirm>
        </div>
      </article>)}
    </section> : <EmptyState icon={<FolderOpenOutlined/>} title={query || filter !== 'all' ? '没有找到匹配的路径' : '还没有快捷路径'} description={query || filter !== 'all' ? '试试其他关键词或分类。' : '把常用文件和文件夹收进来，之后一键打开。'} actions={!query && filter === 'all' ? <Button type="primary" icon={<PlusOutlined/>} onClick={() => void pick(true)}>添加文件夹</Button> : undefined}/>}
    <TagManagerModal open={manage} tags={data.tags} error={error} onClose={() => setManage(false)} onCreate={createTag} onRename={renameTag} onDelete={deleteTag} deleteConfirm={tag => `删除分类“${tag}”？快捷路径不会删除。`}/>
    <Drawer title="添加快捷路径" open={!!draft} onClose={close} width={460} destroyOnHidden footer={<Space style={{ float: 'right' }}><Button onClick={close}>取消</Button><Button type="primary" loading={busy} onClick={() => void add()}>添加</Button></Space>}>
      {draft && <div className="detail-form">
        {error && <Alert type="error" showIcon message={error}/>}
        <label className="form-field"><span>名称</span><Input aria-label="名称" autoFocus value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })}/></label>
        <label className="form-field"><span>路径</span><Input value={draft.path} readOnly/></label>
      </div>}
    </Drawer>
    <Toast message={notice}/>
  </div>
}