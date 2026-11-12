import { useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Skeleton } from 'antd'
import {
  ArrowRightOutlined,
  CalendarOutlined,
  ClockCircleOutlined,
  EditOutlined,
  ExportOutlined,
  FileTextOutlined,
  FolderOpenOutlined,
  GlobalOutlined,
  PlusOutlined,
  PushpinOutlined,
  ToolOutlined,
} from '@ant-design/icons'
import type { Navigate } from './navigation'
import './styles/today.css'

type Assignment = { day: string; topic: { category: string; title: string; prompt: string }; page_id: string | null }
type Page = { id: string; title: string; updated_at: string }
type RecentEntry = Page & { is_essay: boolean }
type Recent = Page & { section: 'writing' | 'essays'; timestamp: number }
type Pin = { id: string; label: string; description: string; action: () => Promise<unknown> }
type LaunchSection = 'websites' | 'folders' | 'tools'
type RecentFilter = 'all' | Recent['section']
type Resource<T> = { state: 'loading' | 'error' } | { state: 'ready'; data: T }

const dateFormat = new Intl.DateTimeFormat('zh-CN', { year: 'numeric', month: 'long', day: 'numeric', weekday: 'long' })
const timestampFormat = new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' })
const shortDate = new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric' })
const relativeTime = new Intl.RelativeTimeFormat('zh-CN', { numeric: 'always' })

function greeting(date: Date) {
  const hour = date.getHours()
  if (hour < 6) return '夜深了'
  if (hour < 12) return '早上好'
  if (hour < 14) return '中午好'
  if (hour < 18) return '下午好'
  return '晚上好'
}

function parseEditTime(value: string) {
  // SQLite stores UTC timestamps without an offset; imported ISO timestamps
  // may already carry one. Parse both before computing relative time.
  return Date.parse(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(\.\d+)?$/.test(value) ? value.replace(' ', 'T') + 'Z' : value)
}

function editTime({ updated_at, timestamp }: Recent) {
  if (!Number.isFinite(timestamp)) return { label: updated_at }
  const date = new Date(timestamp)
  const minutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60_000))
  const label = minutes < 1 ? '刚刚'
    : minutes < 60 ? relativeTime.format(-minutes, 'minute')
    : minutes < 1_440 ? relativeTime.format(-Math.floor(minutes / 60), 'hour')
    : minutes < 10_080 ? relativeTime.format(-Math.floor(minutes / 1_440), 'day')
    : shortDate.format(date)
  return { label, title: timestampFormat.format(date), dateTime: date.toISOString() }
}

function useResource<T>(load: () => Promise<T>) {
  const [resource, setResource] = useState<Resource<T>>({ state: 'loading' })
  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    let cancelled = false
    void load().then(data => {
      if (!cancelled) setResource({ state: 'ready', data })
    }).catch(() => {
      if (!cancelled) setResource({ state: 'error' })
    })
    return () => { cancelled = true }
  }, [load, attempt])
  return {
    resource,
    retry() {
      setResource({ state: 'loading' })
      setAttempt(value => value + 1)
    },
  }
}

const loadAssignment = () => invoke<Assignment>('essay_today')
async function loadRecent(): Promise<Recent[]> {
  const entries = await invoke<RecentEntry[]>('writing_recent')
  return entries.map(({ id, title, updated_at, is_essay }): Recent => ({
    id, title, updated_at, timestamp: parseEditTime(updated_at),
    section: is_essay ? 'essays' : 'writing',
  }))
}

function pinned<T extends { id: string }>(items: T[], favorites: string[], describe: (item: T) => Pin): Pin[] {
  const byId = new Map(items.map(item => [item.id, item]))
  return favorites.flatMap(id => {
    const item = byId.get(id)
    return item ? [describe(item)] : []
  })
}

async function loadWebsites() {
  const [items, { favorites }] = await Promise.all([
    invoke<{ id: string; title: string; url: string }[]>('websites_list'),
    invoke<{ favorites: string[] }>('website_data'),
  ])
  return pinned(items, favorites, item => {
    let description = '网站'
    try { description = new URL(item.url).hostname || description } catch { /* Keep the type label for nonstandard addresses. */ }
    return { id: item.id, label: item.title, description, action: () => invoke('websites_open', { url: item.url }) }
  })
}

async function loadPaths() {
  const { items, favorites } = await invoke<{ items: { id: string; name: string; path: string; is_dir: boolean }[]; favorites: string[] }>('shortcut_data')
  return pinned(items, favorites, item => ({
    id: item.id, label: item.name || item.path, description: item.is_dir ? '文件夹' : '文件',
    action: () => invoke('shortcut_open', { path: item.path }),
  }))
}

async function loadTools() {
  const { items, favorites } = await invoke<{ items: { id: string; name: string; path: string }[]; favorites: string[] }>('tool_data')
  return pinned(items, favorites, item => ({
    id: item.id, label: item.name, description: '应用程序', action: () => invoke('tools_launch', { path: item.path }),
  }))
}

const launchGroups = [
  { section: 'websites', label: '网站收藏', icon: <GlobalOutlined/>, load: loadWebsites },
  { section: 'folders', label: '快捷路径', icon: <FolderOpenOutlined/>, load: loadPaths },
  { section: 'tools', label: '小工具', icon: <ToolOutlined/>, load: loadTools },
] satisfies { section: LaunchSection; label: string; icon: React.ReactNode; load: () => Promise<Pin[]> }[]

function NotebookArt() {
  return <svg className="today-notebook" viewBox="0 0 276 248" fill="none" aria-hidden="true" focusable="false">
    <path d="M76 200a96 96 0 1 1 145 9" stroke="currentColor" strokeOpacity=".22" strokeWidth="1.2"/>
    <path d="M92 208a79 79 0 1 1 114 2" stroke="currentColor" strokeOpacity=".15" strokeWidth="1.2"/>
    <circle cx="191" cy="65" r="29" className="today-notebook-sun"/>
    <path d="M47 70v24m-12-12h24M228 121v14m-7-7h14M51 176h11m-5.5-5.5v11" stroke="currentColor" strokeOpacity=".55" strokeWidth="1.4" strokeLinecap="round"/>
    <path d="M81 223h150m-140 8h104" stroke="currentColor" strokeOpacity=".22" strokeWidth="1.2" strokeLinecap="round"/>
    <g transform="rotate(-11 142 150)" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round">
      <rect x="85" y="87" width="125" height="138" rx="9" className="today-notebook-base"/>
      <rect x="77" y="79" width="125" height="138" rx="9" className="today-notebook-cover"/>
      <path d="M99 79v138" strokeOpacity=".45"/>
      <path d="M73 104h12m-12 27h12m-12 27h12m-12 27h12" strokeLinecap="round"/>
      <rect x="116" y="104" width="62" height="39" rx="4" className="today-notebook-label"/>
      <path d="M128 116h30m-30 8h39m-39 8h21" strokeOpacity=".55" strokeLinecap="round"/>
      <path d="M118 166h56m-56 12h44m-44 12h51" strokeOpacity=".36" strokeLinecap="round"/>
      <path d="M171 79h13v32l-6.5-4.5-6.5 4.5V79Z" className="today-notebook-ribbon" stroke="none"/>
    </g>
    <g transform="rotate(18 224 173)" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round">
      <path d="M220 120a4 4 0 0 1 8 0v89l-4 12-4-12v-89Z" className="today-notebook-cover"/>
      <path d="M220 130h8m-8 79h8"/>
    </g>
  </svg>
}

function InlineRetry({ text, onRetry }: { text: string; onRetry: () => void }) {
  return <div className="today-inline-state" role="status">
    <p>{text}</p>
    <Button size="small" aria-label="重试" onClick={onRetry}>重试</Button>
  </div>
}

function EssayCard({ navigate }: { navigate: Navigate }) {
  const { resource, retry } = useResource(loadAssignment)
  const today = resource.state === 'ready' ? resource.data : null
  return <section className="today-essay" aria-labelledby="today-essay-title" aria-busy={resource.state === 'loading'}>
    <NotebookArt/>
    <div className="today-essay-copy">
      <div className="today-essay-kicker">
        <span className="today-module-mark" aria-hidden="true"><EditOutlined/></span>
        <h2 id="today-essay-title">今日随笔</h2>
        {today && <span className="today-topic-tag">{today.topic.category}</span>}
      </div>
      {resource.state === 'loading' ? <Skeleton paragraph={{ rows: 2 }} title/>
        : !today ? <InlineRetry text="暂时无法读取今日主题。" onRetry={retry}/>
          : <>
            <h3>{today.topic.title}</h3>
            <p className="today-essay-prompt">{today.topic.prompt}</p>
            <div className="today-essay-actions">
              <Button type="primary" icon={<EditOutlined/>} onClick={() => navigate('essays', today.page_id ? { id: today.page_id } : { intent: 'start-essay' })}>
                {today.page_id ? '继续写随笔' : '开始写随笔'}
              </Button>
              <Button type="text" onClick={() => navigate('essays', { intent: 'essay-history' })}>记录本 <ArrowRightOutlined/></Button>
            </div>
          </>}
    </div>
  </section>
}

const recentFilters: { key: RecentFilter; label: string }[] = [
  { key: 'all', label: '全部' }, { key: 'writing', label: '文档' }, { key: 'essays', label: '随笔' },
]

function RecentCard({ navigate }: { navigate: Navigate }) {
  const { resource, retry } = useResource(loadRecent)
  const [filter, setFilter] = useState<RecentFilter>('all')
  const recent = useMemo(() => resource.state === 'ready'
    ? resource.data.filter(page => filter === 'all' || page.section === filter).slice(0, 4) : [], [filter, resource])
  return <section className="paper-sheet today-recent" aria-labelledby="today-recent-title" aria-busy={resource.state === 'loading'}>
    <header className="today-card-head">
      <h2 id="today-recent-title"><ClockCircleOutlined aria-hidden="true"/> 最近编辑</h2>
      <div className="today-recent-filters" role="group" aria-label="按内容类型筛选最近编辑">
        {recentFilters.map(item => <button type="button" key={item.key} aria-pressed={filter === item.key} onClick={() => setFilter(item.key)}>{item.label}</button>)}
      </div>
    </header>
    {resource.state === 'loading' ? <Skeleton paragraph={{ rows: 4 }} title={false}/>
      : resource.state === 'error' ? <InlineRetry text="暂时无法读取最近编辑。" onRetry={retry}/>
        : recent.length ? <ul className="today-recent-list">
          {recent.map(page => {
            const essay = page.section === 'essays'
            const title = page.title || '未命名文档'
            const time = editTime(page)
            return <li key={page.id}>
              <button type="button" className="today-list-row" title={title} onClick={() => navigate(page.section, { id: page.id })}>
                <span className={`today-module-mark ${essay ? 'is-essay' : ''}`} aria-hidden="true">{essay ? <EditOutlined/> : <FileTextOutlined/>}</span>
                <span className="today-list-title">{title}<small>{essay ? '随笔' : '文档'}</small></span>
                <time dateTime={time.dateTime} title={time.title}>{time.label}</time>
                <ArrowRightOutlined className="today-row-arrow" aria-hidden="true"/>
              </button>
            </li>
          })}
        </ul> : <div className="today-inline-state">
          <p>{filter === 'essays' ? '还没有随笔。' : filter === 'writing' ? '还没有文档。' : '写过的文档和随笔会出现在这里。'}</p>
          <Button size="small" onClick={() => filter === 'essays' ? navigate('essays') : navigate('writing', { intent: 'new-page' })}>
            {filter === 'essays' ? '查看今日主题' : '新建文档'}
          </Button>
        </div>}
  </section>
}

function LaunchCard({ group, navigate, onError }: {
  group: typeof launchGroups[number]; navigate: Navigate; onError: (value: string) => void
}) {
  const { resource, retry } = useResource(group.load)
  const [opening, setOpening] = useState<string | null>(null)
  const items = resource.state === 'ready' ? resource.data : []
  async function open(pin: Pin) {
    if (opening) return
    setOpening(pin.id)
    onError('')
    try { await pin.action() } catch (error) {
      onError(`无法打开「${pin.label}」：${String(error)}`)
    } finally {
      setOpening(null)
    }
  }
  return <section className={`paper-sheet today-launch today-launch--${group.section}`} aria-label={group.label} aria-busy={resource.state === 'loading'}>
    <header className="today-launch-head">
      <span className="today-module-mark" aria-hidden="true">{group.icon}</span>
      <h3>{group.label}</h3>
      {resource.state === 'ready' && <span className="today-launch-count" aria-label={`${items.length} 个置顶`}>{items.length}</span>}
      <button className="today-launch-link" type="button" title={`查看${group.label}`} aria-label={`查看${group.label}`} onClick={() => navigate(group.section)}><ArrowRightOutlined/></button>
    </header>
    {resource.state === 'loading' ? <Skeleton paragraph={{ rows: 1 }} title={false}/>
      : resource.state === 'error' ? <InlineRetry text="无法读取置顶入口。" onRetry={retry}/>
        : items.length ? <ul className="today-launch-list">
          {items.slice(0, 3).map(pin => <li key={pin.id}>
            <button type="button" className="today-pin" title={pin.label} disabled={opening !== null} onClick={() => void open(pin)}>
              <span><strong>{pin.label}</strong><small>{opening === pin.id ? '正在打开…' : pin.description}</small></span>
              <ExportOutlined aria-hidden="true"/>
            </button>
          </li>)}
          {items.length > 3 && <li><button className="today-more-pins" type="button" onClick={() => navigate(group.section)}>查看其余 {items.length - 3} 个 <ArrowRightOutlined/></button></li>}
        </ul> : <div className="today-launch-empty"><p>还没有置顶的入口。</p><Button type="link" size="small" onClick={() => navigate(group.section)}>去{group.label}置顶</Button></div>}
  </section>
}

export default function TodayPage({ navigate }: { navigate: Navigate }) {
  const [date] = useState(() => new Date())
  const [error, setError] = useState('')
  return <div className="content today-page">
    <header className="today-header">
      <div>
        <div className="today-date"><CalendarOutlined aria-hidden="true"/>{dateFormat.format(date).replace('日星期', '日 · 星期')}</div>
        <h1>{greeting(date)}</h1>
        <p>继续写作，或从置顶入口打开常用资料。</p>
      </div>
      <Button icon={<PlusOutlined/>} onClick={() => navigate('writing', { intent: 'new-page' })}>新建文档</Button>
    </header>
    <div className="today-grid">
      <EssayCard navigate={navigate}/>
      <RecentCard navigate={navigate}/>
    </div>
    <section className="today-pinned" aria-labelledby="today-pinned-title">
      <header className="today-section-head">
        <span className="today-module-mark" aria-hidden="true"><PushpinOutlined/></span>
        <div><h2 id="today-pinned-title">置顶入口</h2><p>打开常用的网站、路径和小工具。</p></div>
      </header>
      {error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')}/>}
      <div className="today-launch-grid">
        {launchGroups.map(group => <LaunchCard key={group.section} group={group} navigate={navigate} onError={setError}/>)}
      </div>
    </section>
  </div>
}
