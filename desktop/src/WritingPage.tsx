import { useCallback, useEffect, useEffectEvent, useLayoutEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Dropdown, Input, Modal, Select, Tree, Tooltip, type InputRef, type MenuProps } from 'antd'
import { FileTextOutlined, FolderOutlined, MenuFoldOutlined, MenuUnfoldOutlined, MoreOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import type { DataNode } from 'antd/es/tree'
import MarkdownEditor from './MarkdownEditor'
import useWideLayout from './ui/useWideLayout'
import useWritingAutosave, { type RegisterWritingGuard, type WritingDocument } from './useWritingAutosave'
import WritingSaveStatus from './WritingSaveStatus'
import type { FocusState } from './navigation'

type Book = { id: string; parent_id: string | null; title: string; tags: string[] }
type Entry = Pick<WritingDocument, 'id' | 'book_id' | 'title' | 'tags'>
type Directory = { pages: Entry[]; books: Book[] }
type DirectorySearch = { term: string; matches: Set<string> } | { term: string; error: string }
type Move = { kind: 'book' | 'page'; id: string; title: string }
const rootBook = '__personal_writing__'
const documentActionLabels: Record<string, string> = { export: '导出文档', copy: '另存副本', publish: '发布博客', 'publish-mongo': '发布到文档库' }

function directoryRow(target: EventTarget | null) {
  const row = target instanceof Element ? target.closest('.ant-tree-treenode') : null
  const key = row?.querySelector<HTMLElement>('[data-writing-key]')?.dataset.writingKey
  return row && key ? { row, key } : null
}

export default function WritingPage({ setDirty, registerGuard, focus }: {
  setDirty: (value: boolean) => void; registerGuard?: RegisterWritingGuard; focus?: FocusState | null
}) {
  const [pages, setPages] = useState<Entry[]>([])
  const [books, setBooks] = useState<Book[]>([])
  const [selectedBook, setSelectedBook] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [searchResult, setSearchResult] = useState<DirectorySearch | null>(null)
  const [searchAttempt, setSearchAttempt] = useState(0)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [directoryOpen, setDirectoryOpen] = useState(true)
  const [compactDirectory, setCompactDirectory] = useState<boolean | null>(null)
  const [expandedKeys, setExpandedKeys] = useState(['root'])
  const [treeHeight, setTreeHeight] = useState<number>()
  const [naming, setNaming] = useState<{ book: Book | null } | null>(null)
  const [name, setName] = useState('')
  const [moving, setMoving] = useState<Move | null>(null)
  const [destination, setDestination] = useState('')
  const [removing, setRemoving] = useState<{ id: string; page: boolean } | null>(null)
  const [recoverOpen, setRecoverOpen] = useState(false)
  const layout = useRef<HTMLDivElement>(null)
  const directory = useRef<HTMLElement>(null)
  const treeViewport = useRef<HTMLDivElement>(null)
  const draggedKey = useRef<string | null>(null)
  const highlightedRow = useRef<Element | null>(null)
  const returnToDirectory = useRef(false)
  const documentScroll = useRef<HTMLDivElement>(null)
  const titleInput = useRef<InputRef>(null)
  const titleToFocus = useRef<string | null>(null)
  const editorPane = useRef<HTMLElement>(null)
  const focusEditor = useRef(false)
  const running = useRef(false)
  const handledFocus = useRef<number | null>(null)
  const onSaved = useCallback(({ id, title }: WritingDocument) => setPages(rows => {
    const previous = rows.find(row => row.id === id)
    return previous && previous.title !== title
      ? rows.map(row => row.id === id ? { ...row, title } : row)
      : rows
  }), [])
  const { page: active, status, error: saveError, load, edit, flush, trackInput, snapshot } = useWritingAutosave({ setDirty, registerGuard, onSaved })
  const wide = useWideLayout(layout, 780)
  const showDirectory = wide ? directoryOpen : compactDirectory ?? !active
  const activeId = active?.id
  const term = query.trim().toLocaleLowerCase()
  const currentSearch = searchResult?.term === term ? searchResult : null
  const matches = currentSearch && 'matches' in currentSearch ? currentSearch.matches : null
  const searching = !!term && !currentSearch

  useEffect(() => {
    if (!term) return
    let cancelled = false
    const timer = window.setTimeout(() => {
      void invoke<string[]>('writing_matches', { query: term })
        .then(ids => { if (!cancelled) setSearchResult({ term, matches: new Set(ids) }) })
        .catch(cause => { if (!cancelled) setSearchResult({ term, error: '搜索文档失败：' + String(cause) }) })
    }, 110)
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [term, searchAttempt])

  useLayoutEffect(() => {
    const viewport = treeViewport.current
    if (!viewport) return
    const measure = () => {
      if (viewport.clientHeight > 0) setTreeHeight(viewport.clientHeight)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(viewport)
    return () => observer.disconnect()
  }, [])

  useLayoutEffect(() => {
    if (documentScroll.current) documentScroll.current.scrollTop = 0
  }, [activeId])
  useLayoutEffect(() => {
    if (showDirectory && returnToDirectory.current) {
      returnToDirectory.current = false
      const tree = directory.current?.querySelector<HTMLElement>('[role="tree"]')
      const target = tree ?? directory.current?.querySelector('input')
      target?.focus({ preventScroll: true })
    }
  }, [showDirectory])
  useLayoutEffect(() => {
    if (busy || (!wide && showDirectory)) return
    if (activeId && titleToFocus.current === activeId) {
      titleToFocus.current = null
      titleInput.current?.focus({ cursor: 'all' })
    } else if (focusEditor.current) {
      focusEditor.current = false
      editorPane.current?.focus({ preventScroll: true })
    }
  }, [activeId, busy, showDirectory, wide])

  const refresh = useCallback(async () => {
    const { pages: records, books: directories } = await invoke<Directory>('writing_directory')
    setPages(records)
    setBooks(directories)
    setLoaded(true)
  }, [])
  useEffect(() => {
    void refresh().catch(cause => { setLoaded(true); setError('读取文档失败：' + String(cause)) })
  }, [refresh])

  async function afterSave(label: string, action: () => void | Promise<void>) {
    if (running.current) return
    running.current = true
    setBusy(true)
    setError('')
    try {
      if (await flush()) await action()
    } catch (cause) {
      setError(label + '失败：' + String(cause))
    } finally {
      running.current = false
      setBusy(false)
    }
  }

  function openPage(page: WritingDocument, newTitle = false) {
    titleToFocus.current = newTitle ? page.id : null
    focusEditor.current = !wide && !newTitle
    load(page)
    setSelectedBook(page.book_id === rootBook ? null : page.book_id)
    setCompactDirectory(false)
  }

  async function readPage(id: string) {
    const page = await invoke<WritingDocument | null>('writing_page', { id })
    if (!page) throw new Error('该文档已不存在')
    openPage(page)
  }

  function createPage() {
    return afterSave('新建文档', async () => {
      const page = await invoke<WritingDocument>('writing_create', { title: '未命名文档', bookId: selectedBook })
      openPage(page, true)
      await refresh()
    })
  }

  const openFromNavigation = useEffectEvent((target: FocusState) => {
    if (handledFocus.current === target.nonce) return
    if (target.intent === 'new-page') {
      handledFocus.current = target.nonce
      void createPage()
    } else if (target.id) {
      if (!loaded) return
      const page = pages.find(item => item.id === target.id)
      handledFocus.current = target.nonce
      if (!page) { setError('该文档已不存在，或暂时无法读取。'); return }
      if (snapshot()?.id === page.id) {
        focusEditor.current = !wide
        setCompactDirectory(false)
        return
      }
      setQuery('')
      const path = new Set(['root'])
      let book = books.find(item => item.id === page.book_id)
      while (book && !path.has('book:' + book.id)) {
        path.add('book:' + book.id)
        book = books.find(item => item.id === book?.parent_id)
      }
      setExpandedKeys(keys => [...new Set([...keys, ...path])])
      void afterSave('打开文档', () => readPage(page.id))
    }
  })
  useEffect(() => { if (focus) openFromNavigation(focus) }, [focus, loaded, pages, books])

  function selectTreeItem(key: string) {
    if (key.startsWith('page:')) {
      const id = key.slice(5)
      if (activeId === id) {
        if (!wide) { focusEditor.current = true; setCompactDirectory(false) }
        return
      }
      void afterSave('打开文档', () => readPage(id))
    } else if (key.startsWith('book:')) setSelectedBook(key.slice(5))
    else if (key === 'root') setSelectedBook(null)
  }

  const createBook = useCallback((book: Book | null = null) => {
    setName(book?.title ?? '')
    setNaming({ book })
  }, [])
  function saveBookName() {
    if (!name.trim() || !naming) return
    const title = name.trim()
    void afterSave('保存图书', async () => {
      const parent = selectedBook ? 'book:' + selectedBook : 'root'
      await invoke('books_save', { book: naming.book ? { ...naming.book, title } : { id: crypto.randomUUID(), title, parent_id: selectedBook, tags: [] } })
      setNaming(null)
      await refresh()
      setExpandedKeys(keys => keys.includes(parent) ? keys : [...keys, parent])
    })
  }
  const chooseMove = useCallback((kind: Move['kind'], id: string, title: string) => {
    setDestination('')
    setMoving({ kind, id, title })
  }, [])
  function resetEditor() {
    load(null)
    setCompactDirectory(null)
    setSelectedBook(null)
  }
  function move() {
    if (!moving) return
    void afterSave('移动', async () => {
      if (moving.kind === 'book') await invoke('writing_move_book', { id: moving.id, parent: destination || null })
      else await invoke('writing_move_page', { id: moving.id, target: destination || null })
      setMoving(null)
      resetEditor()
      await refresh()
      setNotice('移动完成')
    })
  }

  const parentByKey = useMemo(() => new Map([
    ...pages.map(page => ['page:' + page.id, page.book_id === rootBook ? 'root' : 'book:' + page.book_id] as const),
    ...books.map(book => ['book:' + book.id, book.parent_id ? 'book:' + book.parent_id : 'root'] as const),
  ]), [pages, books])

  function canDrop(source: string, target: string) {
    const parent = parentByKey.get(source)
    if (busy || !parent || source === target || parent === target) return false
    if (target.startsWith('page:')) return source.startsWith('page:') && parent === parentByKey.get(target)
    for (let ancestor: string | undefined = target; ancestor; ancestor = parentByKey.get(ancestor)) {
      if (ancestor === source) return false
    }
    return target === 'root' || parentByKey.has(target)
  }

  function highlightDrop(row: Element | null) {
    if (row === highlightedRow.current) return
    highlightedRow.current?.removeAttribute('data-writing-drop')
    row?.setAttribute('data-writing-drop', '')
    highlightedRow.current = row
  }

  function endDrag() {
    draggedKey.current = null
    highlightDrop(null)
  }

  function startDrag(event: DragEvent) {
    const source = directoryRow(event.target)
    if (busy || !source || source.key === 'root') { event.preventDefault(); return }
    draggedKey.current = source.key
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('application/x-personaldesk-writing', source.key)
  }

  function dragOver(event: DragEvent) {
    const target = directoryRow(event.target)
    if (!draggedKey.current || !target || !canDrop(draggedKey.current, target.key)) {
      highlightDrop(null)
      return
    }
    event.preventDefault()
    event.dataTransfer.dropEffect = 'move'
    highlightDrop(target.row)
  }

  function drop(event: DragEvent) {
    const source = draggedKey.current
    const target = directoryRow(event.target)?.key
    endDrag()
    if (!source || !target || !canDrop(source, target)) return
    event.preventDefault()
    const grouping = target.startsWith('page:')
    void afterSave(grouping ? '合并文档' : '移动', async () => {
      let expand = target
      if (grouping) {
        const book = await invoke<Book>('writing_group_pages', { ids: [target.slice(5), source.slice(5)] })
        expand = 'book:' + book.id
      } else {
        const parent = target === 'root' ? null : target.slice(5)
        if (source.startsWith('book:')) await invoke('writing_move_book', { id: source.slice(5), parent })
        else await invoke('writing_move_page', { id: source.slice(5), target: parent })
      }
      if (activeId && (source === 'page:' + activeId || target === 'page:' + activeId)) await readPage(activeId)
      await refresh()
      setExpandedKeys(keys => keys.includes(expand) ? keys : [...keys, expand])
      setNotice(grouping ? '已将两篇文档归入新图书' : '移动完成')
    })
  }

  function remove() {
    if (!removing) return
    void afterSave('删除', async () => {
      await invoke('writing_delete', removing)
      setRemoving(null)
      resetEditor()
      await refresh()
    })
  }

  const actions: MenuProps['items'] = [
    { key: 'log', label: '生成今日日志' },
    { key: 'recover', label: '恢复草稿' },
    ...(active ? [
      { type: 'divider' as const },
      { key: 'export', label: '导出 Markdown' },
      { key: 'copy', label: '另存副本' },
      { key: 'publish', label: '发布到博客' },
      { key: 'publish-mongo', label: '发布到文档库' },
      { key: 'move', label: '移动文档' },
      { key: 'delete', label: '删除文档', danger: true },
    ] : []),
  ]
  function handleAction(key: string) {
    if (key === 'recover') { setRecoverOpen(true); return }
    if (key === 'log') {
      void afterSave('打开日志', async () => {
        openPage(await invoke<WritingDocument>('writing_daily_log', { bookId: selectedBook }))
        await refresh()
      })
      return
    }
    if (!active) return
    if (key === 'move') { chooseMove('page', active.id, active.title); return }
    if (key === 'delete') { setRemoving({ id: active.id, page: true }); return }
    const label = documentActionLabels[key]
    if (!label) return
    void afterSave(label, async () => {
      const page = snapshot()
      if (!page) return
      if (key === 'export') {
        const path = await invoke<string | null>('writing_export_markdown', { page })
        if (path) setNotice('文档已导出：' + path)
      } else if (key === 'copy') {
        openPage(await invoke<WritingDocument>('writing_save_copy', { page }))
        await refresh()
        setNotice('已另存为新文档')
      } else if (key === 'publish') setNotice('已发布到博客：' + await invoke<string>('writing_publish_blog', { page }))
      else setNotice('已发布到文档库：' + await invoke<string>('writing_publish_mongo', { id: page.id }))
    })
  }

  const nodes = useMemo<DataNode[]>(() => {
    const documents = new Map<string, DataNode[]>()
    for (const page of pages) {
      if (matches && !matches.has(page.id)) continue
      const items = documents.get(page.book_id) ?? []
      items.push({ key: 'page:' + page.id, icon: <FileTextOutlined/>, isLeaf: true,
        title: <span className="document-tree-name" title={page.title} data-writing-key={'page:' + page.id} draggable>{page.title || '未命名文档'}</span> })
      documents.set(page.book_id, items)
    }
    const groups = new Map<string | null, Book[]>()
    for (const book of books) {
      const parent = book.parent_id || null
      const children = groups.get(parent) ?? []
      children.push(book)
      groups.set(parent, children)
    }
    function bookNodes(parent: string | null): DataNode[] {
      return (groups.get(parent) ?? []).flatMap(book => {
        const children = [...bookNodes(book.id), ...(documents.get(book.id) ?? [])]
        if (matches && !book.title.toLocaleLowerCase().includes(term) && !children.length) return []
        return [{ key: 'book:' + book.id, icon: <FolderOutlined/>, children, title: <div className="document-tree-folder">
          <span className="document-tree-name" title={book.title} data-writing-key={'book:' + book.id} draggable>{book.title}</span>
          <span onClick={event => event.stopPropagation()}>
            <Dropdown trigger={['click']} menu={{
              items: [{ key: 'rename', label: '重命名' }, { key: 'move', label: '移动图书' }, { key: 'delete', label: '删除图书', danger: true }],
              onClick: ({ key }) => {
                if (key === 'rename') createBook(book)
                else if (key === 'move') chooseMove('book', book.id, book.title)
                else setRemoving({ id: book.id, page: false })
              },
            }}><Button type="text" size="small" className="tree-more" aria-label={book.title + '操作'} icon={<MoreOutlined/>}/></Dropdown>
          </span>
        </div> }]
      })
    }
    return [{ key: 'root', title: <span data-writing-key="root">全部文档</span>, icon: <FolderOutlined/>, children: [...bookNodes(null), ...(documents.get(rootBook) ?? [])] }]
  }, [books, pages, matches, term, createBook, chooseMove])
  function toggleDirectory() {
    if (wide) setDirectoryOpen(value => !value)
    else {
      returnToDirectory.current = !showDirectory
      focusEditor.current = showDirectory
      setCompactDirectory(!showDirectory)
    }
  }

  return <div ref={layout} className={`content writing-page ${showDirectory ? '' : 'directory-collapsed'} ${wide ? '' : 'writing-compact'}`}>
    <header className="writing-header">
      <div className="writing-header-title">
        <Tooltip title={showDirectory ? '收起目录' : '打开目录'}>
          <Button type="text" aria-label="切换文档目录" aria-expanded={showDirectory} aria-controls="document-directory"
            icon={showDirectory ? <MenuFoldOutlined/> : <MenuUnfoldOutlined/>} onClick={toggleDirectory}/>
        </Tooltip>
        <h1>文档写作</h1><span className="writing-total">{pages.length} 篇</span>
      </div>
      <div className="writing-header-actions">
        {active && <WritingSaveStatus status={status}/>}
        <Dropdown trigger={['click']} menu={{ items: actions, onClick: ({ key }) => handleAction(key) }}>
          <Button type="text" disabled={busy} title="文档操作" aria-label="文档操作" icon={<MoreOutlined/>}/>
        </Dropdown>
        <Button type="primary" aria-label="新建文档" icon={<PlusOutlined/>} disabled={busy} onClick={() => void createPage()}>新建文档</Button>
      </div>
    </header>
    {(saveError || error) && <Alert className="writing-notice" type="error" showIcon message={saveError || error}
      action={saveError ? <Button size="small" onClick={() => void flush()}>重试</Button> : undefined}
      closable={!saveError} onClose={() => setError('')}/>}
    {notice && <Alert className="writing-notice" type="success" closable message={notice} onClose={() => setNotice('')}/>}
    <div className="writing-layout">
      <aside className="document-directory" id="document-directory" aria-label="文档目录" ref={directory} hidden={!showDirectory}>
        <Input aria-label="搜索文档" prefix={<SearchOutlined/>} placeholder="搜索文档" value={query} onChange={event => setQuery(event.target.value)} allowClear/>
        <div className="directory-caption"><span>我的文档</span><Tooltip title="新建图书">
          <Button type="text" size="small" aria-label="新建图书" icon={<PlusOutlined/>} onClick={() => createBook()}/>
        </Tooltip></div>
        <div className="document-tree" ref={treeViewport} aria-busy={!loaded || searching}
          onDragStart={startDrag} onDragOver={dragOver} onDragEnd={endDrag} onDrop={drop}
          onDragLeave={event => { if (!(event.relatedTarget instanceof Node) || !event.currentTarget.contains(event.relatedTarget)) highlightDrop(null) }}>
          {!loaded || searching ? <p className="writing-loading" role="status">{loaded ? '正在搜索…' : '正在读取文档…'}</p>
          : currentSearch && 'error' in currentSearch ? <div className="collection-list-empty">
            <span role="status">{currentSearch.error}</span>
            <Button size="small" onClick={() => { setSearchResult(null); setSearchAttempt(attempt => attempt + 1) }}>重试</Button>
          </div> : term && !nodes[0].children?.length
          ? <div className="collection-list-empty"><span role="status">没有找到匹配的文档。</span><Button type="link" size="small" onClick={() => setQuery('')}>清除搜索</Button></div>
          : <Tree key={term ? 'search:' + term : 'normal'} height={treeHeight} showIcon blockNode
            selectedKeys={active ? ['page:' + active.id] : selectedBook ? ['book:' + selectedBook] : ['root']}
            expandedKeys={term ? ['root', ...books.map(book => 'book:' + book.id)] : expandedKeys}
            onExpand={keys => setExpandedKeys(keys.map(String))} treeData={nodes} onSelect={(_, info) => selectTreeItem(String(info.node.key))}/>}</div>
      </aside>
      <section className="document-workspace" aria-label="编辑文档" ref={editorPane} tabIndex={-1} hidden={!wide && showDirectory}>
        {active ? <div className="document-scroll" ref={documentScroll}><article className="document-paper writing-sheet">
          <Input ref={titleInput} className="document-title" variant="borderless" aria-label="文档标题" disabled={busy}
            value={active.title} placeholder="未命名文档" onPressEnter={() => editorPane.current?.querySelector<HTMLElement>('.cm-content')?.focus()}
            onChange={event => edit({ title: event.target.value })}/>
          <MarkdownEditor key={active.id} documentId={active.id} label="文档正文" disabled={busy} value={active.body}
            onChange={body => edit({ body })} onPendingInput={trackInput}/>
        </article></div> : !loaded || busy
          ? <p className="writing-loading" role="status">正在打开文档…</p> : <div className="writing-empty">
          <span className="writing-empty-icon"><FileTextOutlined/></span><h2>从一页空白开始</h2>
          <p>新建文档，或从目录打开已有文档。</p>
          <Button type="primary" icon={<PlusOutlined/>} loading={busy} onClick={() => void createPage()}>写一篇文档</Button>
          {!showDirectory && <Button type="link" onClick={toggleDirectory}>打开文档目录</Button>}
        </div>}
      </section>
    </div>
    <Modal title={naming?.book ? '重命名图书' : '新建图书'} open={!!naming} onCancel={() => setNaming(null)} onOk={saveBookName}
      okText="保存" cancelText="取消" confirmLoading={busy}>
      <Input aria-label="图书名称" placeholder="为图书命名" value={name} onChange={event => setName(event.target.value)} onPressEnter={saveBookName}/>
    </Modal>
    <Modal title={removing?.page ? '删除此文档？' : '删除此图书及其子图书和页面？'} open={!!removing}
      onCancel={() => setRemoving(null)} onOk={remove} okText="删除" cancelText="取消" okButtonProps={{ danger: true }} confirmLoading={busy}>
      <p>请确认要删除选中的内容。</p>
    </Modal>
    <Modal title="恢复未完成草稿？" open={recoverOpen} onCancel={() => setRecoverOpen(false)} onOk={() => {
      setRecoverOpen(false)
      void afterSave('恢复草稿', async () => { await invoke('writing_recover_drafts'); await refresh(); setNotice('已检查未完成的草稿') })
    }} okText="恢复" cancelText="取消"><p>恢复内容将保存为独立文档。</p></Modal>
    <Modal title={'移动' + (moving?.kind === 'book' ? '图书' : '文档') + '：' + (moving?.title ?? '')}
      open={!!moving} onCancel={() => setMoving(null)} onOk={move} confirmLoading={busy} okText="移动" cancelText="取消">
      <Select aria-label="目标目录" value={destination} onChange={setDestination} style={{ width: '100%' }}
        options={[{ value: '', label: '文档根目录' }, ...books.filter(book => book.id !== moving?.id).map(book => ({ value: book.id, label: book.title }))]}/>
    </Modal>
  </div>
}
