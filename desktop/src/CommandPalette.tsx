import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  ArrowRightOutlined,
  CloseCircleOutlined,
  CloseOutlined,
  ContactsOutlined,
  EditOutlined,
  FileTextOutlined,
  FolderOpenOutlined,
  GlobalOutlined,
  HomeOutlined,
  InboxOutlined,
  LockOutlined,
  PlusOutlined,
  SearchOutlined,
  SettingOutlined,
  ToolOutlined,
} from '@ant-design/icons'
import type { FocusTarget, Navigate, Section } from './navigation'
import { scoredBySearch, type SearchField } from './searchRank'

type Scope = Exclude<Section, 'today' | 'settings'> | 'all'
type Result = {
  key: string
  section: Section
  target?: FocusTarget
  title: string
  meta: string
  kind: string
  icon: React.ReactNode
  fields?: SearchField[]
}
type Source = { scope: Scope; load: () => Promise<Result[]> }
type IndexedSource = { state: 'error' } | { state: 'ready'; items: Result[] }
type WritingHit = { id: string; title: string; excerpt: string; is_essay: boolean; score: number }
type WritingResults = { hits: WritingHit[]; total: number }
type WritingLookup = { key: string; state: 'error' } | { key: string; state: 'ready'; results: WritingResults }
type Props = { open: boolean; onClose: (reason?: 'navigate') => void; onNavigate: Navigate }

const scopes: { key: Scope; label: string }[] = [
  { key: 'all', label: '全部' },
  { key: 'writing', label: '文档' },
  { key: 'essays', label: '随笔' },
  { key: 'contacts', label: '联系人' },
  { key: 'credentials', label: '账号' },
  { key: 'inventory', label: '物品' },
  { key: 'folders', label: '路径' },
  { key: 'websites', label: '网站' },
  { key: 'tools', label: '工具' },
]

const destinations = {
  today:       { label: '今天',     kind: '首页',   icon: <HomeOutlined/>,       hint: '今日随笔、最近编辑和置顶入口', keywords: '首页 工作台 today' },
  writing:     { label: '文档写作', kind: '文档',   icon: <FileTextOutlined/>,   hint: 'Markdown 文档与图书目录', keywords: '文档 笔记 writing' },
  essays:      { label: '每日随笔', kind: '随笔',   icon: <EditOutlined/>,       hint: '今日主题与记录本', keywords: '随笔 日记 essays' },
  contacts:    { label: '联系人',   kind: '联系人', icon: <ContactsOutlined/>,   hint: '资料与聊天记录', keywords: '通讯录 contacts' },
  credentials: { label: '账号密码', kind: '账号',   icon: <LockOutlined/>,       hint: '账号、密码与备注', keywords: '账户 凭据 credentials' },
  inventory:   { label: '物品清单', kind: '物品',   icon: <InboxOutlined/>,      hint: '物品、位置与购买信息', keywords: '物品 清单 inventory' },
  folders:     { label: '快捷路径', kind: '路径',   icon: <FolderOpenOutlined/>, hint: '文件与文件夹', keywords: '文件夹 路径 folders' },
  websites:    { label: '网站收藏', kind: '网站',   icon: <GlobalOutlined/>,     hint: '常用网站与书签', keywords: '网站 书签 websites' },
  tools:       { label: '小工具',   kind: '工具',   icon: <ToolOutlined/>,       hint: '程序与文件入口', keywords: '工具 程序 tools' },
  settings:    { label: '设置',     kind: '设置',   icon: <SettingOutlined/>,    hint: '外观、数据、连接与更新', keywords: '设置 主题 settings' },
} satisfies Record<Section, { label: string; kind: string; icon: React.ReactNode; hint: string; keywords: string }>

const quick: Result[] = [
  { key: 'action:new-page', section: 'writing', title: '新建文档', meta: '打开一篇空白文档', kind: '操作', icon: <PlusOutlined/>, target: { intent: 'new-page' }, fields: [{ value: '创建 笔记 new' }] },
  { key: 'action:essay-history', section: 'essays', title: '查看随笔记录本', meta: '浏览过去的随笔', kind: '操作', icon: <EditOutlined/>, target: { intent: 'essay-history' }, fields: [{ value: '日记 历史' }] },
  ...(Object.entries(destinations) as [Section, typeof destinations[Section]][]).map(([section, destination]) => ({
    key: 'nav:' + section, section, title: destination.label, meta: destination.hint, kind: '跳转', icon: destination.icon,
    fields: [{ value: destination.keywords }],
  })),
]

function record(section: Exclude<Scope, 'all'>, id: string, title: string, meta: string, fields: SearchField[]): Result {
  return { key: section + ':' + id, section, target: { id }, title, meta, kind: destinations[section].kind, icon: destinations[section].icon, fields }
}

type Contact = { id: string; name: string; school: string; major: string; work: string; phone: string }
type Credential = { id: string; title: string; username: string; website: string; tags: string[] }
type Website = { id: string; title: string; url: string; notes: string }
type Shortcut = { id: string; name: string; path: string }
type Tool = Shortcut & { notes: string }
type InventoryItem = { id: string; name: string; location: string; brand: string; model: string; notes: string; tags: string[] }

const sources: Source[] = [
  { scope: 'contacts', async load() {
    const items = await invoke<Contact[]>('contact_search_items')
    return items.map(item => record('contacts', item.id, item.name, [item.phone, item.work, item.school].filter(Boolean).join(' / ') || '联系人资料',
      [{ value: item.phone, weight: 1.5 }, { value: item.work }, { value: item.school }, { value: item.major }]))
  } },
  { scope: 'credentials', async load() {
    const items = await invoke<Credential[]>('credential_search_items')
    return items.map(item => record('credentials', item.id, item.title, [item.username, item.website].filter(Boolean).join(' / ') || '账号',
      [{ value: item.username, weight: 1.7 }, { value: item.website }, { value: item.tags.join(' ') }]))
  } },
  { scope: 'websites', async load() {
    const items = await invoke<Website[]>('websites_list')
    return items.map(item => record('websites', item.id, item.title, item.url, [{ value: item.url, weight: 1.5 }, { value: item.notes, weight: .6 }]))
  } },
  { scope: 'folders', async load() {
    const data = await invoke<{ items: Shortcut[] }>('shortcut_data')
    return data.items.map(item => record('folders', item.id, item.name || item.path, item.path, [{ value: item.path }]))
  } },
  { scope: 'tools', async load() {
    const data = await invoke<{ items: Tool[] }>('tool_data')
    return data.items.map(item => record('tools', item.id, item.name, item.path, [{ value: item.path }, { value: item.notes, weight: .6 }]))
  } },
  { scope: 'inventory', async load() {
    const items = await invoke<InventoryItem[]>('inventory_list')
    return items.map(item => record('inventory', item.id, item.name, [item.location, item.brand, item.model].filter(Boolean).join(' / ') || '物品',
      [{ value: item.location, weight: 1.5 }, { value: item.brand }, { value: item.model }, { value: item.tags.join(' ') }, { value: item.notes, weight: .6 }]))
  } },
]

const resultLimit = 24

function SearchDialog({ onClose, onNavigate }: Omit<Props, 'open'>) {
  const [query, setQuery] = useState('')
  const [scope, setScope] = useState<Scope>('all')
  const [activeKey, setActiveKey] = useState<string | null>(null)
  const [shouldIndex, setShouldIndex] = useState(false)
  const [index, setIndex] = useState<Partial<Record<number, IndexedSource>>>({})
  const [retry, setRetry] = useState(0)
  const [writingLookup, setWritingLookup] = useState<WritingLookup | null>(null)
  const [writingAttempt, setWritingAttempt] = useState(0)
  const [moving, setMoving] = useState(false)
  const requests = useRef(new Map<number, Promise<Result[]>>())
  const writingQueue = useRef(Promise.resolve())
  const input = useRef<HTMLInputElement>(null)
  const list = useRef<HTMLDivElement>(null)
  const listId = useId()

  useLayoutEffect(() => { input.current?.focus() }, [])
  useEffect(() => {
    if (!shouldIndex) return
    let cancelled = false
    sources.forEach((source, key) => {
      if (scope !== 'all' && source.scope !== scope) return
      let request = requests.current.get(key)
      if (!request) {
        request = source.load()
        requests.current.set(key, request)
      }
      const update = (entry: IndexedSource) => {
        if (!cancelled) setIndex(previous => previous[key]?.state === entry.state ? previous : { ...previous, [key]: entry })
      }
      void request.then(items => update({ state: 'ready', items })).catch(() => update({ state: 'error' }))
    })
    return () => { cancelled = true }
  }, [shouldIndex, scope, retry])

  const term = query.trim()
  const writingKey = term && (scope === 'all' || scope === 'writing' || scope === 'essays') ? JSON.stringify([term, scope, writingAttempt]) : ''
  const writing = writingKey && writingLookup?.key === writingKey ? writingLookup : null
  useEffect(() => {
    if (!writingKey) return
    let cancelled = false
    const timer = window.setTimeout(() => {
      writingQueue.current = writingQueue.current.then(async () => {
        if (cancelled) return
        try {
          const results = await invoke<WritingResults>('writing_search', { query: term, scope: scope === 'all' ? null : scope })
          if (!cancelled) setWritingLookup({ key: writingKey, state: 'ready', results })
        } catch {
          if (!cancelled) setWritingLookup({ key: writingKey, state: 'error' })
        }
      })
    }, 110)
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [writingKey, term, scope])

  const results = useMemo(() => {
    const matchesScope = (item: Result) => scope === 'all' || item.section === scope
    if (!term) {
      const shortcuts = quick.filter(matchesScope)
      return { visible: shortcuts, total: shortcuts.length }
    }
    const records = Object.values(index).flatMap(source => source?.state === 'ready' ? source.items : [])
    const local = scoredBySearch([...records, ...quick].filter(matchesScope), term,
      item => [{ value: item.title, weight: 3 }, ...(item.fields ?? [])])
    const fromWriting = writing?.state === 'ready' ? writing.results : null
    const hits = (fromWriting?.hits ?? []).map(hit => ({
      score: hit.score,
      item: record(hit.is_essay ? 'essays' : 'writing', hit.id, hit.title, hit.excerpt, []),
    }))
    const visible = [...hits, ...local.slice(0, resultLimit)]
      .sort((a, b) => b.score - a.score).slice(0, resultLimit).map(entry => entry.item)
    return { visible, total: local.length + (fromWriting?.total ?? 0) }
  }, [index, scope, term, writing])
  const { visible, total } = results
  const activeIndex = Math.max(0, visible.findIndex(item => item.key === activeKey))
  const active = visible[activeIndex]
  const activeId = active ? listId + '-' + activeIndex : undefined
  const sourceStates = sources.flatMap((source, key) => scope === 'all' || source.scope === scope ? [index[key]?.state] : [])
  if (writingKey) sourceStates.push(writing?.state)
  const loading = !!term && sourceStates.some(state => !state)
  const partial = !!term && sourceStates.includes('error')
  const failed = !!term && sourceStates.every(state => state === 'error')

  useEffect(() => {
    if (activeId) list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest' })
  }, [activeId])

  function updateQuery(value: string) {
    setQuery(value)
    setActiveKey(null)
    if (value.trim()) setShouldIndex(true)
  }

  function retryFailed() {
    const keys = sources.flatMap((source, key) =>
      (scope === 'all' || source.scope === scope) && index[key]?.state === 'error' ? [key] : [])
    if (keys.length) {
      keys.forEach(key => requests.current.delete(key))
      setIndex(previous => {
        const next = { ...previous }
        keys.forEach(key => delete next[key])
        return next
      })
      setRetry(value => value + 1)
    }
    if (writing?.state === 'error') setWritingAttempt(attempt => attempt + 1)
    input.current?.focus()
  }

  async function choose(result: Result | undefined) {
    if (!result || moving) return
    setMoving(true)
    try {
      if (await onNavigate(result.section, result.target) !== false) onClose('navigate')
      else onClose()
    } finally {
      setMoving(false)
    }
  }

  return <div className="palette-overlay" onMouseDown={event => { if (event.target === event.currentTarget) onClose() }}>
    <div className="palette" role="dialog" aria-modal="true" aria-label="全局搜索" onKeyDown={event => {
      if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onClose() }
      if (event.key === 'Tab') {
        const elements = event.currentTarget.querySelectorAll<HTMLElement>('input, button:not([tabindex="-1"]):not(:disabled)')
        const first = elements[0]
        const last = elements[elements.length - 1]
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
      }
    }}>
      <div className="palette-input">
        <SearchOutlined aria-hidden="true"/>
        <input
          ref={input}
          role="combobox"
          aria-label="搜索全部内容"
          aria-autocomplete="list"
          aria-expanded="true"
          aria-controls={listId}
          aria-activedescendant={activeId}
          placeholder="搜索文档、联系人、账号或入口"
          value={query}
          onChange={event => updateQuery(event.target.value)}
          onKeyDown={event => {
            if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return
            if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && visible.length) {
              event.preventDefault()
              const next = (activeIndex + (event.key === 'ArrowDown' ? 1 : -1) + visible.length) % visible.length
              setActiveKey(visible[next].key)
            } else if (event.key === 'Enter') { event.preventDefault(); void choose(active) }
          }}
        />
        {query && <button type="button" className="palette-icon-button" title="清空关键词" aria-label="清空关键词" onClick={() => {
          updateQuery('')
          input.current?.focus()
        }}><CloseCircleOutlined/></button>}
        <button type="button" className="palette-icon-button palette-close" title="关闭搜索（Esc）" aria-label="关闭搜索" onClick={() => onClose()}><CloseOutlined/></button>
      </div>
      <div className="palette-scopes" role="group" aria-label="搜索范围">
        {scopes.map(item => <button key={item.key} className="filter-chip" type="button" aria-pressed={scope === item.key} onClick={() => {
          setScope(item.key)
          setActiveKey(null)
          input.current?.focus()
        }}>{item.label}</button>)}
      </div>
      <div className="palette-results">
        <div className="palette-group" role="status">
          <span>{!term ? '快捷操作与页面' : loading ? '正在搜索…' : total > resultLimit ? `显示 ${resultLimit} / ${total} 条结果` : `${total} 条结果`}</span>
          {partial && <span className="palette-partial">{failed ? '读取失败' : '部分资料未能读取'}</span>}
        </div>
        <div id={listId} ref={list} role="listbox" aria-label="搜索结果" aria-busy={loading}>
          {visible.map((item, position) => <button
            key={item.key}
            type="button"
            role="option"
            id={listId + '-' + position}
            tabIndex={-1}
            aria-selected={position === activeIndex}
            className={`palette-item ${position === activeIndex ? 'active' : ''}`}
            onMouseDown={event => event.preventDefault()}
            onMouseMove={() => { if (position !== activeIndex) setActiveKey(item.key) }}
            onClick={() => void choose(item)}
          >
            <span className="palette-icon" aria-hidden="true">{item.icon}</span>
            <span className="palette-copy"><span className="palette-title">{item.title}</span><span className="palette-meta">{item.meta}</span></span>
            <span className="palette-kind">{item.kind}</span>
            <ArrowRightOutlined className="palette-arrow" aria-hidden="true"/>
          </button>)}
        </div>
        {!visible.length && <div className="palette-empty">
          <SearchOutlined aria-hidden="true"/>
          <strong>{loading ? '正在读取本地资料' : failed ? '暂时无法读取这个范围' : '没有找到匹配内容'}</strong>
          {!loading && !failed && <p>试试更短的关键词，或切换搜索范围。</p>}
        </div>}
        {partial && !loading && <button type="button" className="palette-retry" onClick={retryFailed}>重新读取</button>}
      </div>
      <div className="palette-footer">
        <span><kbd>↑</kbd><kbd>↓</kbd> 选择</span>
        <span><kbd>Enter</kbd> 打开</span>
        <span><kbd>Esc</kbd> 关闭</span>
      </div>
    </div>
  </div>
}

export default function CommandPalette({ open, ...props }: Props) {
  return open ? <SearchDialog {...props}/> : null
}
