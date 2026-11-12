import NativeCloseGuard from './NativeCloseGuard'
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  ContactsOutlined,
  EditOutlined,
  FileTextOutlined,
  FolderOpenOutlined,
  GlobalOutlined,
  HomeOutlined,
  InboxOutlined,
  LockOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  MoonOutlined,
  SearchOutlined,
  SettingOutlined,
  SunOutlined,
  ToolOutlined,
} from '@ant-design/icons'
import TodayPage from './TodayPage'
import CommandPalette from './CommandPalette'
import { apply, isDarkTheme, presets, readTheme, type Theme } from './theme'
import type { FocusState, FocusTarget, Section } from './navigation'
import type { RegisterWritingGuard } from './useWritingAutosave'

const ToolsPage = lazy(() => import('./ToolsPage'))
const InventoryPage = lazy(() => import('./InventoryPage'))
const WebsitesPage = lazy(() => import('./WebsitesPage'))
const ShortcutsPage = lazy(() => import('./ShortcutsPage'))
const EssaysPage = lazy(() => import('./EssaysPage'))
const WritingPage = lazy(() => import('./WritingPage'))
const ContactsPage = lazy(() => import('./ContactsPage'))
const SettingsPage = lazy(() => import('./SettingsPage'))
const CredentialsPage = lazy(() => import('./CredentialsPage'))

const navigation: { caption: string; items: { key: Section; label: string; icon: React.ReactNode }[] }[] = [
  {
    caption: '写作',
    items: [
      { key: 'writing', label: '文档写作', icon: <FileTextOutlined/> },
      { key: 'essays', label: '每日随笔', icon: <EditOutlined/> },
    ],
  },
  {
    caption: '资料',
    items: [
      { key: 'contacts', label: '联系人', icon: <ContactsOutlined/> },
      { key: 'credentials', label: '账号密码', icon: <LockOutlined/> },
      { key: 'inventory', label: '物品清单', icon: <InboxOutlined/> },
    ],
  },
  {
    caption: '入口',
    items: [
      { key: 'folders', label: '快捷路径', icon: <FolderOpenOutlined/> },
      { key: 'websites', label: '网站收藏', icon: <GlobalOutlined/> },
      { key: 'tools', label: '小工具', icon: <ToolOutlined/> },
    ],
  },
  {
    caption: '系统',
    items: [{ key: 'settings', label: '设置', icon: <SettingOutlined/> }],
  },
]

/* Crisp window glyphs instead of text characters. */
const MinimizeGlyph = <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true"><path d="M1.5 5.5h8" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/></svg>
const MaximizeGlyph = <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true"><rect x="1.5" y="1.5" width="8" height="8" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.3"/></svg>
const RestoreGlyph = <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.3"><rect x="1.2" y="3.2" width="6.6" height="6.6" rx="1.3"/><path d="M3.4 3.2V2.4A1.2 1.2 0 0 1 4.6 1.2h4.2a1.2 1.2 0 0 1 1.2 1.2v4.2a1.2 1.2 0 0 1-1.2 1.2h-.8"/></svg>
const CloseGlyph = <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true"><path d="M2 2l7 7M9 2l-7 7" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/></svg>

const sidebarPreference = 'personaldesk.sidebar-compact'

function App({ dark = false }: { dark?: boolean }) {
  const [section, setSection] = useState<Section>('today')
  const [focus, setFocus] = useState<FocusState | null>(null)
  const [workspaceDirty, setWorkspaceDirty] = useState(false)
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [maximized, setMaximized] = useState(false)
  const [switchingTheme, setSwitchingTheme] = useState(false)
  const [compactSidebar, setCompactSidebar] = useState(() => {
    try { return window.localStorage.getItem(sidebarPreference) === 'true' } catch { return false }
  })
  const focusNonce = useRef(0)
  const workspaceRef = useRef<HTMLElement>(null)
  const searchOrigin = useRef<HTMLElement | null>(null)
  const writingGuard = useRef<(() => Promise<boolean>) | null>(null)
  const navigating = useRef(false)
  const registerWritingGuard = useCallback<RegisterWritingGuard>(guard => {
    writingGuard.current = guard
    return () => { if (writingGuard.current === guard) writingGuard.current = null }
  }, [])
  const getWritingGuard = useCallback(() => writingGuard.current, [])

  const openPalette = useCallback((origin?: HTMLElement) => {
    const current = document.activeElement
    searchOrigin.current = origin ?? (current instanceof HTMLElement && current !== document.body ? current : null)
    setPaletteOpen(true)
  }, [])
  const closePalette = useCallback((reason?: 'navigate') => {
    const origin = searchOrigin.current
    setPaletteOpen(false)
    window.requestAnimationFrame(() => {
      if (document.activeElement !== document.body) return
      if (!reason && origin?.isConnected) origin.focus({ preventScroll: true })
      else workspaceRef.current?.focus({ preventScroll: true })
    })
  }, [])

  async function navigate(key: Section, target?: FocusTarget) {
    if (key === section && !target) return true
    if (navigating.current) return false
    const guard = writingGuard.current
    if (guard) {
      navigating.current = true
      try { if (!await guard()) return false } finally { navigating.current = false }
    } else if (workspaceDirty && !window.confirm('内容有未保存的修改，确定离开吗？')) return false
    setWorkspaceDirty(false)
    setSection(key)
    setFocus(target ? { section: key, ...target, nonce: ++focusNonce.current } : null)
    return true
  }

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (!event.isComposing && event.keyCode !== 229 && !event.repeat && !event.altKey
        && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        if (paletteOpen) closePalette()
        else openPalette()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [paletteOpen, openPalette, closePalette])

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    const win = getCurrentWindow()
    let disposed = false
    let unlisten: (() => void) | undefined
    let resizeTimer: number | undefined
    const sync = () => { void win.isMaximized().then(value => { if (!disposed) setMaximized(value) }).catch(() => {}) }
    sync()
    void win.onResized(() => {
      window.clearTimeout(resizeTimer)
      resizeTimer = window.setTimeout(sync, 70)
    }).then(stop => { if (disposed) stop(); else unlisten = stop }).catch(() => {})
    return () => { disposed = true; window.clearTimeout(resizeTimer); unlisten?.() }
  }, [])

  async function toggleTheme() {
    if (switchingTheme) return
    setSwitchingTheme(true)
    try {
      const current = await readTheme()
      const target = presets.find(preset => preset.name === (isDarkTheme(current) ? '素纸' : '夜读'))!.theme
      const next: Theme = { ...current, ...target, background: current.background, zoom: current.zoom, animations: current.animations, surface_opacity: current.surface_opacity, sidebar_opacity: current.sidebar_opacity, image_opacity: current.image_opacity, fit: current.fit }
      const persisted = await invoke<Theme>('theme_save', { theme: next })
      apply(persisted, 'persisted')
    } catch {
      // Theme switching is a convenience; failures keep the current theme.
    } finally {
      setSwitchingTheme(false)
    }
  }

  const sectionFocus = (key: Section) => focus?.section === key ? focus : null
  const sidebarToggleLabel = compactSidebar ? '展开导航' : '收起导航'

  function toggleSidebar() {
    const next = !compactSidebar
    setCompactSidebar(next)
    try { window.localStorage.setItem(sidebarPreference, String(next)) } catch { /* Use the preference for this session. */ }
  }

  return <div className={`window-shell ${compactSidebar ? 'sidebar-compact' : ''}`}>
    {/* Tauri 的 drag-region 脚本统一处理拖动和双击最大化；deep 让标题栏内非按钮区域都可拖动，
        避免再手动 startDragging 导致一次按下触发两次拖动。 */}
    <header className="window-titlebar" data-tauri-drag-region="deep" inert={paletteOpen}>
      <div className="window-brand"><span className="brand-seal">台</span><strong>个人办公台</strong></div>
      <button type="button" className="window-search" aria-label="全局搜索" aria-haspopup="dialog" aria-keyshortcuts="Control+K Meta+K" onClick={event => openPalette(event.currentTarget)}>
        <SearchOutlined/>
        <span className="window-search-label">搜索文档、资料和入口</span>
        <kbd>Ctrl K</kbd>
      </button>
      <div className="window-controls">
        <button onClick={() => void getCurrentWindow().minimize()} aria-label="最小化">{MinimizeGlyph}</button>
        <button onClick={() => void getCurrentWindow().toggleMaximize()} aria-label={maximized ? '还原' : '最大化'}>{maximized ? RestoreGlyph : MaximizeGlyph}</button>
        <button className="close" onClick={() => void getCurrentWindow().close()} aria-label="关闭">{CloseGlyph}</button>
      </div>
    </header>
    <div className="app-shell" inert={paletteOpen}>
      <NativeCloseGuard dirty={workspaceDirty} getWritingGuard={getWritingGuard}/>
      <aside className="sidebar">
        <nav id="desk-navigation" aria-label="主导航">
          <button aria-label="今天" title="今天" aria-current={section === 'today' ? 'page' : undefined} className={`nav-item ${section === 'today' ? 'active' : ''}`} onClick={() => navigate('today')}>
            <span className="nav-icon"><HomeOutlined/></span><span>今天</span>
          </button>
          {navigation.map(group => <div className="sidebar-group" key={group.caption}>
            <div className="sidebar-caption">{group.caption}</div>
            {group.items.map(item => <button
              key={item.key}
              aria-label={item.label}
              title={item.label}
              aria-current={item.key === section ? 'page' : undefined}
              className={`nav-item ${item.key === section ? 'active' : ''}`}
              onClick={() => navigate(item.key)}
            >
              <span className="nav-icon">{item.icon}</span><span>{item.label}</span>
            </button>)}
          </div>)}
        </nav>
        <div className="sidebar-foot">
          <button type="button" className="sidebar-toggle" aria-label={sidebarToggleLabel} aria-expanded={!compactSidebar} aria-controls="desk-navigation" title={sidebarToggleLabel} onClick={toggleSidebar}>
            {compactSidebar ? <MenuUnfoldOutlined/> : <MenuFoldOutlined/>}
            <span>{sidebarToggleLabel}</span>
          </button>
          <button
            className="theme-toggle"
            aria-label={dark ? '切换到浅色主题' : '切换到夜读主题'}
            title={dark ? '切换到浅色主题' : '切换到夜读主题'}
            disabled={switchingTheme}
            onClick={() => void toggleTheme()}
          >{dark ? <SunOutlined/> : <MoonOutlined/>}</button>
        </div>
      </aside>
      <main className="workspace" ref={workspaceRef} tabIndex={-1}>
        <Suspense fallback={<div className="content workspace-loading" role="status">正在打开页面…</div>}>
        {section === 'today' ? <TodayPage navigate={navigate}/> : section === 'credentials' ? <CredentialsPage setWorkspaceDirty={setWorkspaceDirty} focus={sectionFocus('credentials')}/> : section === 'tools' ? <ToolsPage setWorkspaceDirty={setWorkspaceDirty} focus={sectionFocus('tools')}/> : section === 'inventory' ? <InventoryPage setWorkspaceDirty={setWorkspaceDirty} focus={sectionFocus('inventory')}/> : section === 'websites' ? <WebsitesPage setWorkspaceDirty={setWorkspaceDirty} focus={sectionFocus('websites')}/> : section === 'folders' ? <ShortcutsPage setWorkspaceDirty={setWorkspaceDirty} focus={sectionFocus('folders')}/> : section === 'essays' ? <EssaysPage setDirty={setWorkspaceDirty} registerGuard={registerWritingGuard} focus={sectionFocus('essays')}/> : section === 'writing' ? <WritingPage setDirty={setWorkspaceDirty} registerGuard={registerWritingGuard} focus={sectionFocus('writing')}/> : section === 'contacts' ? <ContactsPage setWorkspaceDirty={setWorkspaceDirty} focus={sectionFocus('contacts')}/> : <SettingsPage setWorkspaceDirty={setWorkspaceDirty} dirty={workspaceDirty}/>}
        </Suspense>
      </main>
    </div>
    <CommandPalette open={paletteOpen} onClose={closePalette} onNavigate={navigate}/>
  </div>
}
export default App
