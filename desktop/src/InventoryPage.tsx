import EditableRecordTable from './EditableRecordTable'
import TagManagerModal from './TagManagerModal'
import { rankBySearch } from './searchRank'
import useUnsavedChanges from './useUnsavedChanges'
import EmptyState from './ui/EmptyState'
import PinButton from './ui/PinButton'
import Toast from './ui/Toast'
import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Drawer, Input, InputNumber, Popconfirm, Select, Space } from 'antd'
import { DeleteOutlined, EditOutlined, InboxOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import type { FocusState } from './navigation'

type Item = { id: string; name: string; category: string; quantity: number; location: string; brand: string; model: string; notes: string; tags: string[]; purchase_date: string; disposal_date: string; purchase_price_cents: number | null }
const blank = (): Item => ({ id: crypto.randomUUID(), name: '', category: '', quantity: 1, location: '', brand: '', model: '', notes: '', tags: [], purchase_date: '', disposal_date: '', purchase_price_cents: null })
type InventoryRow = Omit<Item, 'quantity'> & { quantity: string; price: string }

export default function InventoryPage({ setWorkspaceDirty, focus }: {
  setWorkspaceDirty?: (value: boolean) => void
  focus?: FocusState | null
}) {
  const [rowDirty, setRowDirty] = useState(false)
  const [items, setItems] = useState<Item[]>([])
  const [tags, setTags] = useState<string[]>([])
  const [favorites, setFavorites] = useState<string[]>([])
  const [draft, setDraft] = useState<Item | null>(null)
  const [snapshot, setSnapshot] = useState('')
  const [price, setPrice] = useState('')
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [manage, setManage] = useState(false)
  const [busy, setBusy] = useState(false)
  const handledFocus = useRef<number | null>(null)
  const scrollContainer = useRef<HTMLDivElement>(null)

  const dirty = rowDirty || !!(draft && JSON.stringify([draft, price]) !== snapshot)
  useUnsavedChanges(dirty, setWorkspaceDirty)
  useEffect(() => { if (!notice) return; const timer = window.setTimeout(() => setNotice(''), 2400); return () => window.clearTimeout(timer) }, [notice])

  async function refresh() {
    try {
      const [records, categories, pinned] = await Promise.all([
        invoke<Item[]>('inventory_list'),
        invoke<string[]>('inventory_tags'),
        invoke<string[]>('inventory_favorites'),
      ])
      setItems(records)
      setTags(categories)
      setFavorites(pinned)
      setError('')
    } catch (e) {
      setError(`读取物品失败：${String(e)}`)
    }
  }

  useEffect(() => { void refresh() }, [])

  const visible = useMemo(() => {
    const byTag = items.filter(item => filter === 'all' || (filter === 'untagged' ? !item.tags.length : item.tags.includes(filter)))
    return rankBySearch(byTag, query, item => [
      { value: item.name, weight: 3 },
      { value: item.location, weight: 1 },
      { value: item.brand, weight: 1 },
      { value: item.model, weight: 1 },
      { value: item.tags.join(' '), weight: 1.2 },
      { value: item.notes, weight: 0.6 },
    ], (a, b) => Number(favorites.includes(b.id)) - Number(favorites.includes(a.id)))
  }, [items, filter, query, favorites])

  useEffect(() => {
    if (!focus?.id || handledFocus.current === focus.nonce) return
    const item = items.find(record => record.id === focus.id)
    if (item) {
      handledFocus.current = focus.nonce
      edit(item)
    }
  }, [focus?.nonce, focus?.id, items])

  function edit(item: Item) {
    setDraft({ ...item, tags: [...item.tags] })
    setPrice(item.purchase_price_cents === null ? '' : (item.purchase_price_cents / 100).toFixed(2))
    setSnapshot(JSON.stringify([item, item.purchase_price_cents === null ? '' : (item.purchase_price_cents / 100).toFixed(2)]))
    setError('')
  }

  function close() {
    if (draft && JSON.stringify([draft, price]) !== snapshot && !window.confirm('物品信息有未保存的修改，确定关闭吗？')) return
    setDraft(null)
  }

  async function save() {
    if (!draft || busy) return
    if (!draft.name.trim()) { setError('请填写物品名称'); return }
    if (!Number.isSafeInteger(draft.quantity) || draft.quantity < 0) { setError('数量需要填写大于等于 0 的整数'); return }
    if (price && !/^\d+(\.\d{1,2})?$/.test(price)) { setError('价格需要为非负金额，最多两位小数'); return }
    const cents = price ? Math.round(Number(price) * 100) : null
    if (cents !== null && !Number.isSafeInteger(cents)) { setError('价格超出范围'); return }
    setBusy(true)
    try {
      await invoke('inventory_save', { entry: { ...draft, category: draft.tags.join(' · '), purchase_price_cents: cents } })
      setDraft(null)
      await refresh()
      setNotice('物品已保存')
    } catch (e) {
      setError(`保存物品失败：${String(e)}`)
    } finally {
      setBusy(false)
    }
  }

  async function saveRow(row: InventoryRow) {
    const { price: rowPrice, quantity, ...rest } = row
    if (!row.name.trim()) throw new Error('请填写物品名称')
    if (!/^\d+$/.test(quantity) || !Number.isSafeInteger(Number(quantity))) throw new Error('数量需要填写大于等于 0 的整数')
    if (rowPrice && !/^\d+(\.\d{1,2})?$/.test(rowPrice)) throw new Error('价格需要为非负金额，最多两位小数')
    const cents = rowPrice ? Math.round(Number(rowPrice) * 100) : null
    if (cents !== null && !Number.isSafeInteger(cents)) throw new Error('价格超出范围')
    await invoke('inventory_save', { entry: { ...rest, quantity: Number(quantity), category: row.tags.join(' · '), purchase_price_cents: cents } })
    await refresh()
  }

  async function action(command: string, args: Record<string, unknown>) {
    try { await invoke(command, args); await refresh(); return true } catch (e) { setError(`操作失败：${String(e)}`); return false }
  }
  async function createTag(name: string) { return action('inventory_tag_create', { name }) }
  async function renameTag(original: string, name: string) {
    const ok = await action('inventory_tag_rename', { original, name })
    if (ok && filter === original) setFilter(name)
    return ok
  }
  async function deleteTag(name: string) {
    const ok = await action('inventory_tag_delete', { name })
    if (ok && filter === name) setFilter('all')
    return ok
  }

  return <div className="content inventory-page" ref={scrollContainer}>
    <header className="page-header">
      <div><h1>物品清单</h1><p>记录物品、位置和购买信息</p></div>
      <Space><Button type="primary" icon={<PlusOutlined/>} disabled={rowDirty} onClick={() => edit(blank())}>添加物品</Button></Space>
    </header>
    {error && <Alert type="error" showIcon closable onClose={() => setError('')} message={error} style={{ marginBottom: 16 }}/>}
    <section className="toolbar">
      <Input className="search-antd" prefix={<SearchOutlined/>} allowClear aria-label="搜索物品" placeholder="搜索名称、位置、品牌或备注" value={query} onChange={event => setQuery(event.target.value)}/>
      <div className="toolbar-controls">
        <Select aria-label="按分类筛选" style={{ width: 140 }} value={filter} onChange={setFilter} options={[{ value: 'all', label: '全部分类' }, { value: 'untagged', label: '未分类' }, ...tags.map(tag => ({ value: tag, label: tag }))]}/>
        <Button type="text" disabled={rowDirty} onClick={() => { setError(''); setManage(true) }}>管理分类</Button>
      </div>
    </section>
    <div className="section-heading"><div><h2>物品账本</h2><span>共 {visible.length} 条</span></div></div>
    {visible.length ? <EditableRecordTable<InventoryRow>
      scrollContainer={scrollContainer}
      items={visible.map(item => ({ ...item, quantity: String(item.quantity), price: item.purchase_price_cents === null ? '' : (item.purchase_price_cents / 100).toFixed(2) }))}
      fields={[
        { key: 'name', title: '名称' },
        { key: 'quantity', title: '数量' },
        { key: 'location', title: '位置' },
        { key: 'brand', title: '品牌' },
        { key: 'model', title: '型号' },
        { key: 'price', title: '价格（元）' },
        { key: 'purchase_date', title: '购买日期' },
        { key: 'disposal_date', title: '处置日期' },
        { key: 'notes', title: '备注', multiline: true },
      ]}
      tags={tags}
      label={item => item.name}
      onDirty={setRowDirty}
      onSave={saveRow}
      actions={(item, editing) => <>
        <PinButton pinned={favorites.includes(item.id)} label={item.name} disabled={editing} onClick={() => void action('inventory_favorite', { id: item.id, value: !favorites.includes(item.id) })}/>
        <Button type="text" icon={<EditOutlined/>} aria-label="详情" title="详情" disabled={editing} onClick={() => edit(items.find(record => record.id === item.id)!)}/>
        <Popconfirm title={`删除“${item.name}”？`} okText="删除" cancelText="取消" onConfirm={() => void action('inventory_delete', { id: item.id }).then(ok => { if (ok) setNotice('物品已删除') })}><Button type="text" danger icon={<DeleteOutlined/>} aria-label="删除" title="删除" disabled={editing}/></Popconfirm>
      </>}
    /> : <EmptyState icon={<InboxOutlined/>} title={query || filter !== 'all' ? '没有找到匹配的物品' : '还没有物品记录'} description={query || filter !== 'all' ? '试试其他关键词或分类。' : '添加第一件物品，记录它的位置和购买信息。'} actions={!query && filter === 'all' ? <Button type="primary" icon={<PlusOutlined/>} onClick={() => edit(blank())}>添加物品</Button> : undefined}/>}
    <TagManagerModal open={manage} tags={tags} error={error} onClose={() => setManage(false)} onCreate={createTag} onRename={renameTag} onDelete={deleteTag} deleteConfirm={tag => `确定删除分类“${tag}”？物品不会被删除。`}/>
    <Drawer title="物品详情" open={!!draft} onClose={close} width={480} destroyOnHidden footer={<Space style={{ float: 'right' }}><Button onClick={close}>取消</Button><Button type="primary" loading={busy} onClick={() => void save()}>保存物品</Button></Space>}>
      {draft && <div className="detail-form">
        {error && <Alert type="error" showIcon message={error}/>}
        <label className="form-field"><span>名称 <em>*</em></span><Input aria-label="名称" value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })}/></label>
        <label className="form-field"><span>分类</span><Select mode="multiple" aria-label="分类" style={{ width: '100%' }} value={draft.tags} onChange={value => setDraft({ ...draft, tags: value })} options={tags.map(tag => ({ label: tag, value: tag }))}/></label>
        <div className="form-grid">
          <label className="form-field"><span>数量</span><InputNumber aria-label="数量" min={0} precision={0} value={draft.quantity} onChange={value => setDraft({ ...draft, quantity: value ?? 0 })}/></label>
          <label className="form-field"><span>位置</span><Input aria-label="位置" value={draft.location} onChange={event => setDraft({ ...draft, location: event.target.value })}/></label>
        </div>
        <div className="form-grid">
          <label className="form-field"><span>品牌</span><Input aria-label="品牌" value={draft.brand} onChange={event => setDraft({ ...draft, brand: event.target.value })}/></label>
          <label className="form-field"><span>型号</span><Input aria-label="型号" value={draft.model} onChange={event => setDraft({ ...draft, model: event.target.value })}/></label>
          <label className="form-field"><span>购买日期</span><Input aria-label="购买日期" value={draft.purchase_date} placeholder="YYYY-MM-DD" onChange={event => setDraft({ ...draft, purchase_date: event.target.value })}/></label>
          <label className="form-field"><span>处置日期</span><Input aria-label="处置日期" value={draft.disposal_date} placeholder="YYYY-MM-DD" onChange={event => setDraft({ ...draft, disposal_date: event.target.value })}/></label>
        </div>
        <label className="form-field"><span>购买价格（元）</span><Input aria-label="购买价格（元）" value={price} onChange={event => setPrice(event.target.value)}/></label>
        <label className="form-field"><span>备注</span><Input.TextArea aria-label="备注" rows={3} value={draft.notes} onChange={event => setDraft({ ...draft, notes: event.target.value })}/></label>
      </div>}
    </Drawer>
    <Toast message={notice}/>
  </div>
}
