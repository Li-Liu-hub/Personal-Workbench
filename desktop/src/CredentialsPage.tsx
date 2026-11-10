import EditableRecordTable from './EditableRecordTable'
import useUnsavedChanges from './useUnsavedChanges'
import { useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Button, Input, Modal, Select, Segmented } from 'antd'
import { AppstoreOutlined, UnorderedListOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'

type Credential = { id: string; title: string; username: string; password: string; website: string; notes: string; tags: string[] }
const blank = (): Credential => ({ id: crypto.randomUUID(), title: '', username: '', password: '', website: '', notes: '', tags: [] })

// The account vault keeps its own state and Tauri calls; App only routes to it and observes dirty/count.
export default function CredentialsPage({ setWorkspaceDirty, onCountChange }: { setWorkspaceDirty?: (dirty: boolean) => void; onCountChange?: (count: number) => void }) {
  const [items, setItems] = useState<Credential[]>([])
  const [favorites, setFavorites] = useState<string[]>([])
  const [tags, setTags] = useState<string[]>([])
  const [draft, setDraft] = useState<Credential | null>(null)
  const [tagsText, setTagsText] = useState('')
  const [draftSnapshot, setDraftSnapshot] = useState('')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [manageTags, setManageTags] = useState(false)
  const [newTag, setNewTag] = useState('')
  const [mode, setMode] = useState<'grid' | 'list'>('grid')
  const [rowDirty, setRowDirty] = useState(false)
  const credentialDirty = rowDirty || !!(draft && (JSON.stringify(draft) !== draftSnapshot || tagsText.trim()))
  useUnsavedChanges(credentialDirty, setWorkspaceDirty)
  useEffect(() => { onCountChange?.(items.length) }, [items.length, onCountChange])
  const filtered = useMemo(() => items.filter(item => (filter === 'all' || (filter === 'untagged' ? !item.tags.length : item.tags.includes(filter))) && [item.title, item.username, item.password, item.notes, item.website, ...item.tags].some(v => v.toLowerCase().includes(query.trim().toLowerCase()))).sort((a, b) => Number(favorites.includes(b.id)) - Number(favorites.includes(a.id))), [items, filter, query, favorites])

  async function refresh() {
    try {
      const [records, pinned, categories] = await Promise.all([invoke<Credential[]>('credentials_list'), invoke<string[]>('credential_favorites'), invoke<string[]>('credential_tags')])
      setItems(records); setFavorites(pinned); setTags(categories); setError('')
    } catch (e) { setError(`读取账号失败：${String(e)}`) }
  }
  // Initial load is inlined with a cancellable chain so the first render never writes state after unmount.
  useEffect(() => {
    let cancelled = false
    void Promise.all([invoke<Credential[]>('credentials_list'), invoke<string[]>('credential_favorites'), invoke<string[]>('credential_tags')])
      .then(([records, pinned, categories]) => { if (cancelled) return; setItems(records); setFavorites(pinned); setTags(categories); setError('') })
      .catch(e => { if (!cancelled) setError(`读取账号失败：${String(e)}`) })
    void invoke<'grid' | 'list'>('credential_view_get').then(mode => { if (!cancelled) setMode(mode) }).catch(e => { if (!cancelled) setError(`读取视图设置失败：${String(e)}`) })
    return () => { cancelled = true }
  }, [])
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2200); return () => window.clearTimeout(timer) }, [notice])

  async function saveRow(entry: Credential) {
    if (!entry.title.trim()) throw new Error('请填写名称')
    await invoke('credentials_save', { entry: { ...entry, title: entry.title.trim() } })
    await refresh()
  }
  async function changeMode(next: 'grid' | 'list') {
    if (rowDirty && !window.confirm('列表有未保存的修改，确定切换视图吗？')) return
    try { await invoke('credential_view_set', { mode: next }); setMode(next); setRowDirty(false) }
    catch (e) { setError(`保存视图设置失败：${String(e)}`) }
  }
  function edit(item: Credential) { setDraft({ ...item, tags: [...item.tags] }); setTagsText(''); setDraftSnapshot(JSON.stringify(item)); setError('') }
  function closeDraft() {
    if (!draft) return
    const current = { ...draft, tags: tagsText.trim() ? [...draft.tags, tagsText.trim()] : draft.tags }
    if (JSON.stringify(current) !== draftSnapshot && !window.confirm('账号信息有未保存的修改，确定关闭吗？')) return
    setDraft(null); setError('')
  }
  async function save() {
    if (!draft || !draft.title.trim()) { setError('请填写名称'); return }
    const selected = tagsText.trim() && !draft.tags.includes(tagsText.trim()) ? [...draft.tags, tagsText.trim()] : draft.tags
    try { await invoke('credentials_save', { entry: { ...draft, title: draft.title.trim(), tags: selected } }); setDraft(null); await refresh(); setNotice('账号已保存') }
    catch (e) { setError(`保存失败：${String(e)}`) }
  }
  async function remove(item: Credential) {
    if (!window.confirm(`确定删除“${item.title}”吗？`)) return
    try { await invoke('credentials_delete', { id: item.id }); await refresh(); setNotice('账号已删除') }
    catch (e) { setError(`删除失败：${String(e)}`) }
  }
  async function copy(label: string, value: string) {
    try { await invoke('copy_value', { value }); setNotice(`${label}已复制`); setError('') }
    catch { setError('复制失败，请检查剪贴板权限') }
  }
  async function toggleFavorite(item: Credential) {
    try { await invoke('credential_favorite', { id: item.id, value: !favorites.includes(item.id) }); await refresh() }
    catch (e) { setError(`操作失败：${String(e)}`) }
  }
  function toggleDraftTag(tag: string) {
    if (!draft) return
    setDraft({ ...draft, tags: draft.tags.includes(tag) ? draft.tags.filter(t => t !== tag) : [...draft.tags, tag] })
  }
  async function createTag() {
    if (!newTag.trim()) return
    try { await invoke('credential_tag_create', { name: newTag }); setNewTag(''); await refresh() } catch (e) { setError(`添加分类失败：${String(e)}`) }
  }
  async function renameTag(original: string) {
    const name = window.prompt('修改分类名称', original)
    if (!name || name === original) return
    try { await invoke('credential_tag_rename', { original, name }); if (filter === original) setFilter(name); await refresh() } catch (e) { setError(`修改分类失败：${String(e)}`) }
  }
  async function deleteTag(name: string) {
    if (!window.confirm(`确定删除分类“${name}”吗？账号本身不会删除。`)) return
    try { await invoke('credential_tag_delete', { name }); if (filter === name) setFilter('all'); await refresh() } catch (e) { setError(`删除分类失败：${String(e)}`) }
  }

  return <>
    <div className="content">
      <header className="page-header">
        <div><div className="eyebrow">PERSONAL VAULT</div><h1>账号密码管理</h1><p>妥善整理你的账号、密码与备注</p></div>
        <div className="toolbar-controls"><Button type="primary" size="large" disabled={rowDirty} icon={<PlusOutlined/>} onClick={() => edit(blank())}>新增账号</Button></div>
      </header>
      {error && <div className="error" role="alert">{error}<button onClick={() => setError('')} aria-label="关闭提示">×</button></div>}
      <section className="toolbar">
        <Input className="search-antd" prefix={<SearchOutlined/>} aria-label="搜索账号" placeholder="搜索名称、账号、密码或备注" value={query} onChange={e => setQuery(e.target.value)} allowClear/>
        <div className="toolbar-controls">
          <Select aria-label="按分类筛选" value={filter} onChange={setFilter} options={[{value:'all',label:'全部分类'},{value:'untagged',label:'未分类'},...tags.map(tag=>({value:tag,label:tag}))]} style={{width:140}}/>
          <Button type="text" disabled={rowDirty} onClick={() => setManageTags(true)}>管理分类</Button>
          <Segmented aria-label="视图模式" value={mode} onChange={value => void changeMode(value as 'grid' | 'list')} options={[{value:'grid',label:<AppstoreOutlined aria-label="卡片视图"/>},{value:'list',label:<UnorderedListOutlined aria-label="列表视图"/>}]}/>
        </div>
      </section>
      <div className="section-heading"><div><h2>账号列表</h2><span>共 {filtered.length} 条</span></div><span className="section-hint">点击名称查看和编辑</span></div>
      {mode==='list' ? <EditableRecordTable<Credential> items={filtered} fields={[{key:'title',title:'名称'},{key:'username',title:'账号'},{key:'password',title:'密码'},{key:'website',title:'网址或应用'},{key:'notes',title:'备注',multiline:true}]} tags={tags} label={i=>i.title} onDirty={setRowDirty} onSave={saveRow} actions={(item,editing)=><><Button disabled={editing} onClick={()=>void toggleFavorite(item)}>{favorites.includes(item.id)?'取消置顶':'置顶'}</Button><Button disabled={editing} onClick={()=>edit(item)}>详情</Button><Button disabled={editing} danger onClick={()=>void remove(item)}>删除</Button><Button onClick={()=>void copy('账号',item.username)}>复制账号</Button><Button onClick={()=>void copy('密码',item.password)}>复制密码</Button><Button onClick={()=>void copy('备注',item.notes)}>复制备注</Button></>}/> : filtered.length ? <section className={`credentials ${mode}`} aria-label="账号列表">{filtered.map(item => <article className="credential-card" key={item.id}><div className="card-head"><button className="card-title" onClick={() => edit(item)}><span className="account-icon">▣</span><span className="title-content"><strong>{item.title}</strong><small>{item.tags.length ? item.tags.join(' · ') : '未分类'}</small></span></button><div className="card-actions"><button className={`icon-button favorite ${favorites.includes(item.id) ? 'on' : ''}`} title={favorites.includes(item.id) ? '取消置顶' : '置顶'} aria-label={favorites.includes(item.id) ? '取消置顶' : '置顶'} onClick={() => void toggleFavorite(item)}>{favorites.includes(item.id) ? '★' : '☆'}</button><button className="icon-button" title="删除" aria-label={`删除${item.title}`} onClick={() => void remove(item)}>×</button></div></div><div className="card-fields">{([['账号', item.username], ['密码', item.password], ['备注', item.notes]] as const).map(([label,value]) => <div className="field" key={label}><div className="field-content"><span className="field-label">{label}</span><p title={value}>{value || '—'}</p></div><button className="copy-button" disabled={!value} onClick={() => void copy(label,value)} aria-label={`复制${label}`}>复制</button></div>)}</div></article>)}<button className="add-card" onClick={() => edit(blank())}><span>＋</span><strong>添加账号</strong><small>记录新的账号信息</small></button></section> : <div className="empty"><div className="empty-icon">▣</div><strong>{query || filter !== 'all' ? '没有找到匹配的账号' : '还没有账号记录'}</strong><p>{query || filter !== 'all' ? '试试其他关键词或分类' : '添加第一条记录，让信息井然有序'}</p>{!query && filter === 'all' && <button className="primary" onClick={() => edit(blank())}>＋ 添加账号</button>}</div>}
    </div>
    {notice && <div className="toast" role="status">✓ {notice}</div>}
    {manageTags && <div className="overlay"><section className="dialog" role="dialog" aria-modal="true" aria-labelledby="tags-title"><div className="dialog-header"><div><span className="dialog-kicker">整理账号</span><h2 id="tags-title">管理分类</h2></div><button className="icon-button" onClick={() => setManageTags(false)} aria-label="关闭">×</button></div><div className="dialog-body">{error && <p className="dialog-error" role="alert">{error}</p>}<div className="tag-create"><input value={newTag} onChange={e => setNewTag(e.target.value)} placeholder="输入分类名称" aria-label="分类名称"/><button className="primary" onClick={() => void createTag()}>添加</button></div>{tags.map(tag => <div className="tag-row" key={tag}><span>{tag}</span><div><button onClick={() => void renameTag(tag)}>重命名</button><button onClick={() => void deleteTag(tag)}>删除</button></div></div>)}{!tags.length && <p className="tag-empty">还没有分类，可在这里添加。</p>}</div><div className="dialog-footer"><button className="secondary" onClick={() => setManageTags(false)}>完成</button></div></section></div>}
    <Modal open={!!draft} title={draft && items.some(i => i.id === draft.id) ? '编辑账号' : '添加账号'} onCancel={closeDraft} onOk={() => void save()} okText="保存账号" cancelText="取消" destroyOnHidden>
      {draft && <div className="dialog-body ant-dialog-body">{error && <p className="dialog-error" role="alert">{error}</p>}<label>名称 <em>*</em><Input autoFocus value={draft.title} onChange={e => setDraft({...draft,title:e.target.value})} placeholder="例如：工作邮箱"/></label><div className="form-row"><label>账号<Input value={draft.username} onChange={e => setDraft({...draft,username:e.target.value})} placeholder="用户名或邮箱"/></label><label>密码<div style={{display:'flex',gap:8}}><Input style={{flex:1}} value={draft.password} onChange={e => setDraft({...draft,password:e.target.value})} placeholder="输入密码"/><Button onClick={() => void (async()=>{try{const password=await invoke<string>('password_generate');setDraft({...draft,password});setNotice('已生成随机密码');setError('')}catch(e){setError(`生成密码失败：${String(e)}`)}})()}>生成</Button></div></label></div><label>网址或应用<Input value={draft.website} onChange={e => setDraft({...draft,website:e.target.value})} placeholder="可选"/></label><label>分类<Input value={tagsText} onChange={e => setTagsText(e.target.value)} placeholder="输入一个新分类，或点击下方选择已有分类"/></label>{tags.length > 0 && <div className="tag-choices" aria-label="已有分类">{tags.map(tag => <button type="button" key={tag} className={draft.tags.includes(tag) ? 'chosen' : ''} onClick={() => toggleDraftTag(tag)}>{tag}</button>)}</div>}<label>备注<Input.TextArea rows={4} value={draft.notes} onChange={e => setDraft({...draft,notes:e.target.value})} placeholder="添加需要记住的其他信息"/></label></div>}
    </Modal>
  </>
}