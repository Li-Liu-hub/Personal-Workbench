import ThemeEditor from './ThemeEditor'
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Collapse, Descriptions, Input, Spin, Modal, Space, Switch } from 'antd'
import {
  BgColorsOutlined,
  CloudServerOutlined,
  DatabaseOutlined,
  HistoryOutlined,
  LockOutlined,
  RocketOutlined,
} from '@ant-design/icons'
import { getCurrentWindow } from '@tauri-apps/api/window'
type Release = { version: string; title: string; changes: string[] }
type Info = { version: string; data_directory: string; releases: Release[] }
type Update = { version: string | null; notes: string | null }
type VaultInfo = { configured: boolean; host: string; port: number; user: string; database: string }
type MongoInfo = { configured: boolean; host: string; port: number; user: string; auth_database: string; database: string; collection: string }

type Pane = 'appearance' | 'data' | 'vault' | 'mongo' | 'startup' | 'releases'
const panes: { key: Pane; label: string; icon: React.ReactNode }[] = [
  { key: 'appearance', label: '外观', icon: <BgColorsOutlined/> },
  { key: 'data', label: '本地数据', icon: <DatabaseOutlined/> },
  { key: 'vault', label: '加密库', icon: <LockOutlined/> },
  { key: 'mongo', label: '文档库', icon: <CloudServerOutlined/> },
  { key: 'startup', label: '启动与更新', icon: <RocketOutlined/> },
  { key: 'releases', label: '版本历史', icon: <HistoryOutlined/> },
]

export default function SettingsPage({ setWorkspaceDirty, dirty = false }: { setWorkspaceDirty?: (dirty: boolean) => void; dirty?: boolean }) {
  const [pane, setPane] = useState<Pane>('appearance')
  const [info, setInfo] = useState<Info | null>(null), [error, setError] = useState(''), [update, setUpdate] = useState<Update | null>(null), [checking, setChecking] = useState(false), [installing, setInstalling] = useState(false), [autostart, setAutostart] = useState(false)
  const [vault, setVault] = useState<VaultInfo | null>(null), [host, setHost] = useState('10.0.0.13'), [port, setPort] = useState('3306'), [user, setUser] = useState(''), [password, setPassword] = useState(''), [database, setDatabase] = useState('personaldesk_vault'), [testing, setTesting] = useState(false), [savingVault, setSavingVault] = useState(false), [syncing, setSyncing] = useState(false), [vaultNotice, setVaultNotice] = useState('')
  const [mongo, setMongo] = useState<MongoInfo | null>(null), [mongoHost, setMongoHost] = useState('10.0.0.13'), [mongoPort, setMongoPort] = useState('27017'), [mongoUser, setMongoUser] = useState(''), [mongoPassword, setMongoPassword] = useState(''), [mongoAuthDatabase, setMongoAuthDatabase] = useState('admin'), [mongoDatabase, setMongoDatabase] = useState('personaldesk'), [mongoCollection, setMongoCollection] = useState('documents'), [mongoTesting, setMongoTesting] = useState(false), [mongoSaving, setMongoSaving] = useState(false), [mongoNotice, setMongoNotice] = useState('')

  async function loadVault() { try { const value = await invoke<VaultInfo>('vault_mysql_info'); setVault(value); setHost(value.host); setPort(String(value.port)); setUser(value.user); setDatabase(value.database) } catch (e) { setError(`读取加密库连接设置失败：${String(e)}`) } }
  async function loadMongo() { try { const value = await invoke<MongoInfo>('mongo_info'); setMongo(value); setMongoHost(value.host); setMongoPort(String(value.port)); setMongoUser(value.user); setMongoAuthDatabase(value.auth_database); setMongoDatabase(value.database); setMongoCollection(value.collection) } catch (e) { setError(`读取 MongoDB 文档库设置失败：${String(e)}`) } }
  useEffect(() => { void invoke<Info>('settings_info').then(setInfo).catch(e => setError(`读取设置信息失败：${String(e)}`)); void invoke<boolean>('settings_autostart_get').then(setAutostart).catch(e => setError(`读取开机启动设置失败：${String(e)}`)); void loadVault(); void loadMongo() }, [])
  async function changeAutostart(value: boolean) { try { await invoke('settings_autostart_set', { enabled: value }); setAutostart(value) } catch (e) { setError(`修改开机启动设置失败：${String(e)}`) } }
  async function testVault() { setTesting(true); setVaultNotice(''); try { const portNumber = Number(port); if (!Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) throw new Error('端口必须是 1-65535 的整数'); if (password) await invoke('vault_mysql_test', { host, port: portNumber, user, password, database }); else await invoke('vault_mysql_test_saved'); setVaultNotice('连接成功'); setError('') } catch (e) { setError(`测试加密库连接失败：${String(e)}`) } finally { setTesting(false) } }
  async function saveVault() { if (!password) { setError('首次保存直连配置需要填写 MySQL 密码；密码只在本机用 DPAPI 保存。'); return } setSavingVault(true); setVaultNotice(''); try { const portNumber = Number(port); if (!Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) throw new Error('端口必须是 1-65535 的整数'); await invoke('vault_mysql_save', { host, port: portNumber, user, password, database }); setPassword(''); await loadVault(); setVaultNotice('直连配置已保存；后续同步会使用私网直连'); setError('') } catch (e) { setError(`保存加密库连接失败：${String(e)}`) } finally { setSavingVault(false) } }
  async function clearVault() { if (!window.confirm('清除本机保存的直连配置？清除后会暂时回退到 SSH 兼容通道。')) return; try { await invoke('vault_mysql_clear'); setPassword(''); await loadVault(); setVaultNotice('本机直连配置已清除') } catch (e) { setError(`清除加密库连接失败：${String(e)}`) } }
  async function syncVault() { setSyncing(true); setVaultNotice(''); try { const changed = await invoke<boolean>('vault_sync_now'); setVaultNotice(changed ? '已同步到服务器' : '当前没有待同步的改动') } catch (e) { setError(`同步加密库失败：${String(e)}`) } finally { setSyncing(false) } }
  function mongoPortNumber() { const portNumber = Number(mongoPort); if (!Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) throw new Error('端口必须是 1-65535 的整数'); return portNumber }
  function mongoConnection() { return { host: mongoHost, port: mongoPortNumber(), user: mongoUser, password: mongoPassword, authDatabase: mongoAuthDatabase, database: mongoDatabase, collection: mongoCollection } }
  async function testMongo() { setMongoTesting(true); setMongoNotice(''); try { if (mongoPassword) await invoke('mongo_test', mongoConnection()); else await invoke('mongo_test_saved'); setMongoNotice('文档库连接成功'); setError('') } catch (e) { setError(`测试 MongoDB 文档库连接失败：${String(e)}`) } finally { setMongoTesting(false) } }
  async function saveMongo() { if (!mongoPassword) { setError('首次保存 MongoDB 文档库配置需要填写密码；密码只在本机用 DPAPI 保存。'); return } setMongoSaving(true); setMongoNotice(''); try { await invoke('mongo_save', mongoConnection()); setMongoPassword(''); await loadMongo(); setMongoNotice('MongoDB 文档库配置已保存'); setError('') } catch (e) { setError(`保存 MongoDB 文档库配置失败：${String(e)}`) } finally { setMongoSaving(false) } }
  async function clearMongo() { if (!window.confirm('清除本机保存的 MongoDB 文档库配置？清除后文档写作页将无法发布到文档库。')) return; try { await invoke('mongo_clear'); setMongoPassword(''); await loadMongo(); setMongoNotice('MongoDB 文档库配置已清除') } catch (e) { setError(`清除 MongoDB 文档库配置失败：${String(e)}`) } }
  async function checkUpdate() { setChecking(true); try { setUpdate(await invoke<Update>('settings_check_update')); setError('') } catch (e) { setError(`检查更新失败：${String(e)}`) } finally { setChecking(false) } }
  function confirmInstall() { if (dirty) { setError('请先保存或取消当前修改，再安装更新。'); return } Modal.confirm({ title: `安装个人办公台 v${update?.version}？`, content: update?.notes || '更新前会备份本地数据库，安装后自动重启。', okText: '安装并重启', cancelText: '稍后', onOk: async () => { setInstalling(true); try { await invoke('settings_install_update'); await getCurrentWindow().close() } catch (e) { setError(`启动更新失败：${String(e)}`); throw e } finally { setInstalling(false) } } }) }

  return <div className="content">
    <header className="page-header"><div><h1>设置</h1><p>外观、本地数据、连接与版本更新</p></div></header>
    {error && <Alert type="error" message={error} showIcon closable onClose={() => setError('')} style={{ marginBottom: 16 }}/>}
    {info ? <div className="settings-shell">
      <nav className="settings-nav" aria-label="设置分类">
        {panes.map(item => <button key={item.key} aria-label={item.label} className={pane === item.key ? 'active' : ''} onClick={() => setPane(item.key)}>{item.icon}<span>{item.label}</span></button>)}
      </nav>
      <div className="settings-panel">
        <section className="paper-sheet settings-section" hidden={pane !== 'appearance'}>
          <h2>外观</h2>
          <p>选择一套预设，或继续微调颜色、背景与界面缩放。修改会立即预览，保存后长期生效。</p>
          <ThemeEditor setWorkspaceDirty={setWorkspaceDirty}/>
        </section>
        {pane === 'data' && <section className="paper-sheet settings-section">
          <h2>本地数据</h2>
          <p>数据库和文档图片保存在本机数据目录；建议定期把整个目录加入你自己的备份计划。</p>
          <Descriptions column={1} items={[
            { key: 'version', label: '当前基线版本', children: `v${info.version}` },
            { key: 'path', label: '数据目录', children: info.data_directory },
          ]}/>
        </section>}
        {pane === 'vault' && <section className="paper-sheet settings-section">
          <h2>服务器加密库</h2>
          <p>账号密码等私密资料只在服务器保存由本机密钥加密后的密文；写作文档与配图从 0.15.76 起保存在本机数据目录。MySQL 密码使用当前 Windows 用户的 DPAPI 保存在本机，不写入源码或本机明文文件。</p>
          {vault?.configured ? <Alert type="success" showIcon message={`已配置直连 ${vault.host}:${vault.port}，数据库 ${vault.database}`}/> : <Alert type="warning" showIcon message="尚未配置直连，当前回退到 SSH 兼容通道"/>}
          <div className="settings-grid">
            <Input addonBefore="私网地址" aria-label="MySQL 私网地址" value={host} onChange={e => setHost(e.target.value)} placeholder="例如 10.0.0.13"/>
            <Input addonBefore="端口" aria-label="MySQL 端口" value={port} onChange={e => setPort(e.target.value)} placeholder="3306"/>
            <Input addonBefore="用户名" aria-label="MySQL 用户名" value={user} onChange={e => setUser(e.target.value)} placeholder="MySQL 桌面账号"/>
            <Input addonBefore="数据库" aria-label="MySQL 数据库" value={database} onChange={e => setDatabase(e.target.value)} placeholder="personaldesk_vault"/>
            <Input.Password addonBefore="密码" aria-label="MySQL 密码" className="settings-grid-wide" value={password} onChange={e => setPassword(e.target.value)} placeholder={vault?.configured ? '已保存；修改配置时重新输入' : '首次配置需要填写'}/>
          </div>
          <Space wrap className="settings-actions"><Button loading={testing} onClick={() => void testVault()}>测试连接</Button><Button type="primary" loading={savingVault} onClick={() => void saveVault()}>保存直连配置</Button><Button loading={syncing} onClick={() => void syncVault()}>立即同步</Button><Button danger onClick={() => void clearVault()}>清除本机配置</Button></Space>
          {vaultNotice && <Alert style={{ marginTop: 12 }} type="success" showIcon message={vaultNotice} closable onClose={() => setVaultNotice('')}/>}
        </section>}
        {pane === 'mongo' && <section className="paper-sheet settings-section">
          <h2>MongoDB 文档库</h2>
          <p>用于把写作文档及配图发布到私网 MongoDB。连接密码使用当前 Windows 用户的 DPAPI 保存在本机，不写入源码或本机明文文件。</p>
          {mongo?.configured ? <Alert type="success" showIcon message={`已配置文档库 ${mongo.host}:${mongo.port}，目标 ${mongo.database}.${mongo.collection}`}/> : <Alert type="warning" showIcon message="尚未配置文档库；配置后可在文档写作页发布文档"/>}
          <div className="settings-grid">
            <Input aria-label="MongoDB 地址" addonBefore="私网地址" value={mongoHost} onChange={e => setMongoHost(e.target.value)} placeholder="例如 10.0.0.13"/>
            <Input aria-label="MongoDB 端口" addonBefore="端口" value={mongoPort} onChange={e => setMongoPort(e.target.value)} placeholder="27017"/>
            <Input aria-label="MongoDB 认证库" addonBefore="认证库" value={mongoAuthDatabase} onChange={e => setMongoAuthDatabase(e.target.value)} placeholder="admin"/>
            <Input aria-label="MongoDB 数据库" addonBefore="数据库" value={mongoDatabase} onChange={e => setMongoDatabase(e.target.value)} placeholder="personaldesk"/>
            <Input aria-label="MongoDB 用户名" addonBefore="用户名" value={mongoUser} onChange={e => setMongoUser(e.target.value)} placeholder="MongoDB 账号"/>
            <Input aria-label="MongoDB 集合" addonBefore="集合" value={mongoCollection} onChange={e => setMongoCollection(e.target.value)} placeholder="documents"/>
            <Input.Password aria-label="MongoDB 密码" addonBefore="密码" className="settings-grid-wide" value={mongoPassword} onChange={e => setMongoPassword(e.target.value)} placeholder={mongo?.configured ? '已保存；修改配置时重新输入' : '首次配置需要填写'}/>
          </div>
          <Space wrap className="settings-actions"><Button aria-label="测试 MongoDB 文档库连接" loading={mongoTesting} onClick={() => void testMongo()}>测试连接</Button><Button aria-label="保存 MongoDB 文档库配置" type="primary" loading={mongoSaving} onClick={() => void saveMongo()}>保存</Button><Button aria-label="清除 MongoDB 文档库配置" danger onClick={() => void clearMongo()}>清除</Button></Space>
          {mongoNotice && <Alert style={{ marginTop: 12 }} type="success" showIcon message={mongoNotice} closable onClose={() => setMongoNotice('')}/>}
        </section>}
        {pane === 'startup' && <>
          <section className="paper-sheet settings-section">
            <h2>开机启动</h2>
            <p>登录 Windows 后自动启动个人办公台。</p>
            <Space><Switch aria-label="开机启动" checked={autostart} onChange={value => void changeAutostart(value)}/><span className="meta">登录 Windows 后自动启动</span></Space>
          </section>
          <section className="paper-sheet settings-section">
            <h2>版本更新</h2>
            <p>从 GitHub Releases 检查并安装新版本；安装前会备份本地数据库。</p>
            <Space wrap><Button loading={checking} onClick={() => void checkUpdate()}>检查更新</Button>{update?.version && <Button type="primary" loading={installing} disabled={dirty} onClick={confirmInstall}>安装 v{update.version} 并重启</Button>}</Space>
            {update && !update.version && <Alert type="success" message="当前已是最新版本" style={{ marginTop: 12 }}/>}
            {update?.version && <Alert type="info" message={`发现 v${update.version}`} description={update.notes} style={{ marginTop: 12 }}/>}
          </section>
        </>}
        {pane === 'releases' && <section className="paper-sheet settings-section">
          <h2>版本历史</h2>
          <p>记录每次发布的主要变化。</p>
          <Collapse items={info.releases.map(release => ({ key: release.version, label: `v${release.version} · ${release.title}`, children: <ul>{release.changes.map((change, index) => <li key={index}>{change}</li>)}</ul> }))}/>
        </section>}
      </div>
    </div> : !error && <Spin/>}
  </div>
}