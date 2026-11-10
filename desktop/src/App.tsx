import NativeCloseGuard from './NativeCloseGuard'
import { useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { FolderOpenOutlined, LockOutlined, FileTextOutlined, EditOutlined, ContactsOutlined, InboxOutlined, GlobalOutlined, ToolOutlined, SettingOutlined } from '@ant-design/icons'
import './App.css'
import ToolsPage from './ToolsPage'
import InventoryPage from './InventoryPage'
import WebsitesPage from './WebsitesPage'
import ShortcutsPage from './ShortcutsPage'
import EssaysPage from './EssaysPage'
import WritingPage from './WritingPage'
import ContactsPage from './ContactsPage'
import SettingsPage from './SettingsPage'
import CredentialsPage from './CredentialsPage'

const symbols: Record<string, React.ReactNode> = { credentials:<LockOutlined/>, folders:<FolderOpenOutlined/>, writing:<FileTextOutlined/>, essays:<EditOutlined/>, contacts:<ContactsOutlined/>, inventory:<InboxOutlined/>, websites:<GlobalOutlined/>, tools:<ToolOutlined/>, settings:<SettingOutlined/> }
const navigation = [ ['folders','快捷路径'], ['credentials','账号密码管理'], ['writing','文档写作'], ['essays','每日随笔'], ['contacts','联系人'], ['inventory','物品清单'], ['websites','网站收藏'], ['tools','小工具'], ['settings','设置'] ] as const
type Section = typeof navigation[number][0]

function App() {
  const [section, setSection] = useState<Section>('folders')
  const [workspaceDirty, setWorkspaceDirty] = useState(false)
  const [credentialCount, setCredentialCount] = useState(0)
  const activeLabel = navigation.find(([key]) => key === section)?.[1] ?? ''
  function navigate(key: Section) {
    if (key === section) return
    if (workspaceDirty && !window.confirm('内容有未保存的修改，确定离开吗？')) return
    setWorkspaceDirty(false)
    setSection(key)
  }
  // 显式调用 startDragging：不依赖注入的 drag-region 脚本，按钮区域不触发拖动。
  function beginDrag(event: React.MouseEvent) {
    if (event.button !== 0) return
    if ((event.target as HTMLElement).closest('button')) return
    void getCurrentWindow().startDragging().catch(() => {})
  }
  function toggleMaximizeOnDoubleClick(event: React.MouseEvent) {
    if ((event.target as HTMLElement).closest('button')) return
    void getCurrentWindow().toggleMaximize().catch(() => {})
  }
  return <div className="window-shell">
    <header className="window-titlebar" data-tauri-drag-region onMouseDown={beginDrag} onDoubleClick={toggleMaximizeOnDoubleClick}>
      <div className="window-title" data-tauri-drag-region><span className="window-title-mark"/>个人办公台</div>
      <div className="window-controls">
        <button onClick={() => void getCurrentWindow().minimize()} aria-label="最小化">─</button>
        <button onClick={() => void getCurrentWindow().toggleMaximize()} aria-label="最大化">□</button>
        <button className="close" onClick={() => void getCurrentWindow().close()} aria-label="关闭">×</button>
      </div>
    </header>
    <div className="app-shell">
    <NativeCloseGuard dirty={workspaceDirty}/>
    <aside className="sidebar"><div className="brand"><span className="brand-mark"><i/><i/><i/><i/></span><div><strong>个人办公台</strong><small>工作与生活，有序记录</small></div></div><div className="sidebar-caption">日常工作</div><nav>{navigation.map(([key,label]) => <button key={key} aria-label={label} className={`nav-item ${key === section ? 'active' : ''}`} onClick={() => navigate(key)}><span className="nav-icon">{symbols[key]}</span><span>{label}</span>{key === 'credentials' && <span className="nav-count">{credentialCount}</span>}</button>)}</nav><div className="sidebar-foot"><span className="status-dot"/>本地工作空间</div></aside>
    <main className="workspace"><div className="topline" data-tauri-drag-region><div><span className="breadcrumb">日常工作</span><span className="crumb-separator">/</span><span>{activeLabel}</span></div><span className="top-caption">个人办公台</span></div>
      {section === 'credentials' ? <CredentialsPage setWorkspaceDirty={setWorkspaceDirty} onCountChange={setCredentialCount}/> : section === "tools" ? <ToolsPage setWorkspaceDirty={setWorkspaceDirty}/> : section === "inventory" ? <InventoryPage setWorkspaceDirty={setWorkspaceDirty}/> : section === "websites" ? <WebsitesPage setWorkspaceDirty={setWorkspaceDirty}/> : section === "folders" ? <ShortcutsPage setWorkspaceDirty={setWorkspaceDirty}/> : section === "essays" ? <EssaysPage dirty={workspaceDirty} setDirty={setWorkspaceDirty}/> : section === "writing" ? <WritingPage dirty={workspaceDirty} setDirty={setWorkspaceDirty}/> : section === "contacts" ? <ContactsPage setWorkspaceDirty={setWorkspaceDirty}/> : <SettingsPage setWorkspaceDirty={setWorkspaceDirty} dirty={workspaceDirty}/>}
    </main>
    </div>
  </div>
}
export default App