import { useCallback, useEffect, useEffectEvent, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Input, Segmented, Skeleton } from 'antd'
import { ArrowLeftOutlined, EditOutlined } from '@ant-design/icons'
import EmptyState from './ui/EmptyState'
import MarkdownEditor from './MarkdownEditor'
import WritingSaveStatus from './WritingSaveStatus'
import useWritingAutosave, { type RegisterWritingGuard, type WritingDocument } from './useWritingAutosave'
import type { FocusState } from './navigation'

type Assignment = { day: string; topic: { id: string; category: string; title: string; prompt: string }; page_id: string | null }
type Summary = { day: string; title: string; topic: string; page_id: string }

export default function EssaysPage({ setDirty, registerGuard, focus }: {
  setDirty: (dirty: boolean) => void; registerGuard?: RegisterWritingGuard; focus?: FocusState | null
}) {
  const [today, setToday] = useState<Assignment | null>(null)
  const [history, setHistory] = useState<Summary[]>([])
  const [query, setQuery] = useState('')
  const [mode, setMode] = useState<'today' | 'history'>(() => focus?.intent === 'essay-history' ? 'history' : 'today')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const running = useRef(false)
  const handledFocus = useRef<number | null>(null)
  const editor = useRef<HTMLElement>(null)
  const { page, status, error: saveError, load, edit, flush, trackInput, snapshot } = useWritingAutosave({
    setDirty, registerGuard, untitled: '未命名随笔',
  })

  const loadToday = useCallback(async () => {
    try { setToday(await invoke<Assignment>('essay_today')); setError('') }
    catch (cause) { setError('读取今日主题失败：' + String(cause)) }
  }, [])
  useEffect(() => { void loadToday() }, [loadToday])
  useEffect(() => {
    if (mode !== 'history' || page) return
    let cancelled = false
    void invoke<Summary[]>('essay_history', { query }).then(rows => {
      if (!cancelled) { setHistory(rows); setError('') }
    }).catch(cause => { if (!cancelled) setError('读取记录失败：' + String(cause)) })
    return () => { cancelled = true }
  }, [mode, query, page])

  async function afterSave(label: string, action: () => Promise<void>) {
    if (running.current) return
    running.current = true
    setBusy(true)
    setError('')
    try { if (await flush()) await action() }
    catch (cause) { setError(label + '失败：' + String(cause)) }
    finally { running.current = false; setBusy(false) }
  }
  function start() {
    if (!today) return
    void afterSave('打开随笔', async () => load(await invoke<WritingDocument>('essay_start', { day: today.day })))
  }
  function open(id: string) {
    if (snapshot()?.id === id) return
    void afterSave('打开随笔', async () => {
      const document = await invoke<WritingDocument | null>('essay_page', { id })
      if (!document) throw new Error('随笔已不存在')
      load(document)
    })
  }
  function leave() {
    void afterSave('返回', async () => { load(null); await loadToday() })
  }
  function reroll() {
    if (!today) return
    void afterSave('更换主题', async () => {
      setToday(await invoke<Assignment>('essay_reroll', { day: today.day, expected: today.topic.id }))
    })
  }
  const openFromNavigation = useEffectEvent((target: FocusState) => {
    if (handledFocus.current === target.nonce) return
    if (target.intent === 'start-essay') {
      if (!today) return
      handledFocus.current = target.nonce
      start()
    } else if (target.intent === 'essay-history') {
      handledFocus.current = target.nonce
      load(null)
      setMode('history')
    } else if (target.id) {
      handledFocus.current = target.nonce
      open(target.id)
    }
  })
  useEffect(() => { if (focus) openFromNavigation(focus) }, [focus, today])

  if (page) return <div className="content writing-page essay-editor-page">
    <header className="writing-header">
      <div className="writing-header-title">
        <Button type="text" icon={<ArrowLeftOutlined aria-hidden="true"/>} disabled={busy} onClick={leave}>返回</Button>
        <h1>每日随笔</h1>
      </div>
      <WritingSaveStatus status={status}/>
    </header>
    {(saveError || error) && <Alert className="writing-notice" type="error" showIcon message={saveError || error}
      action={saveError ? <Button size="small" onClick={() => void flush()}>重试</Button> : undefined}
      closable={!saveError} onClose={() => setError('')}/>}
    <div className="document-scroll"><article className="document-paper writing-sheet" ref={editor}>
      <Input className="document-title" variant="borderless" aria-label="标题" placeholder="随笔标题" disabled={busy}
        value={page.title} onChange={event => edit({ title: event.target.value })}
        onPressEnter={() => editor.current?.querySelector<HTMLElement>('.cm-content')?.focus()}/>
      <MarkdownEditor key={page.id} documentId={page.id} label="正文" disabled={busy} value={page.body}
        onChange={body => edit({ body })} onPendingInput={trackInput}/>
    </article></div>
  </div>

  return <div className="content">
    <header className="page-header">
      <div><h1>每日随笔</h1><p>每天一个主题，记录想法与生活</p></div>
      <Segmented value={mode} onChange={value => setMode(value)} options={[
        { value: 'today' as const, label: '今日主题' }, { value: 'history' as const, label: '记录本' },
      ]}/>
    </header>
    {error && <Alert type="error" showIcon closable message={error} onClose={() => setError('')}/>}
    {mode === 'today' ? <section className="essay-today">
      {today ? <>
        <div className="essay-today-date">{today.day}</div>
        <span className="essay-today-topic">{today.topic.category}</span>
        <h2>{today.topic.title}</h2>
        <p className="essay-today-prompt">{today.topic.prompt}</p>
        <div className="essay-today-actions">
          <Button type="primary" loading={busy} onClick={start}>{today.page_id ? '继续写作' : '开始写作'}</Button>
          {!today.page_id && <Button disabled={busy} onClick={reroll}>换个主题</Button>}
        </div>
      </> : error ? <Button onClick={() => void loadToday()}>重试</Button> : <Skeleton paragraph={{ rows: 4 }} title={false}/>}
    </section> : <div className="essay-history">
      <Input className="essay-history-search" aria-label="搜索随笔" placeholder="搜索日期、主题、标题或正文"
        value={query} onChange={event => setQuery(event.target.value)} allowClear/>
      {history.length ? history.map(row => <button type="button" className="essay-history-row" key={row.page_id} onClick={() => open(row.page_id)}>
        <span className="essay-history-day">{row.day}</span>
        <span className="essay-history-copy"><strong>{row.title}</strong><small>{row.topic}</small></span>
      </button>) : <EmptyState icon={<EditOutlined/>} title={query ? '没有找到匹配的随笔' : '还没有随笔记录'}
        description={query ? '试试其他关键词。' : '从今天的主题开始写。'}/>}
    </div>}
  </div>
}
