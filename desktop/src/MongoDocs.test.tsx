// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import SettingsPage from './SettingsPage'
import WritingPage from './WritingPage'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
})
afterEach(() => { cleanup(); invoke.mockReset(); vi.restoreAllMocks() })

const mongoConfig = { configured: false, host: '10.0.0.13', port: 27017, user: '文档账号', auth_database: 'admin', database: 'personaldesk', collection: 'documents' }
const theme = { accent: [58, 101, 89], text: [40, 51, 47], muted: [123, 133, 128], canvas: [248, 249, 248], sidebar: [244, 246, 244], surface: [255, 255, 255], input: [247, 248, 247], border: [227, 231, 228], button_text: [255, 255, 255], sidebar_text: [98, 109, 103], surface_opacity: 1, sidebar_opacity: 1, image_opacity: 0.7, content_overlay: 1, zoom: 1, animations: true, background: null, fit: 'Cover' }
const page = { id: 'p', book_id: '__personal_writing__', title: '正文', body: '内容', tags: [], revision: 1, updated_at: '' }

function mockSettings(mongo: unknown) {
  invoke.mockImplementation((name: string) => {
    if (name === 'settings_info') return Promise.resolve({ version: '0.15.75', data_directory: 'D:\\测试', releases: [] })
    if (name === 'settings_autostart_get') return Promise.resolve(false)
    if (name === 'theme_get') return Promise.resolve(theme)
    if (name === 'vault_mysql_info') return Promise.resolve({ configured: false, host: '10.0.0.13', port: 3306, user: '', database: 'personaldesk_vault' })
    if (name === 'mongo_info') return Promise.resolve(mongo)
    return Promise.resolve(undefined)
  })
}
function mockWriting(extra?: (name: string, args?: Record<string, unknown>) => unknown) {
  invoke.mockImplementation((name: string, args?: Record<string, unknown>) => {
    if (name === 'writing_list') return Promise.resolve([page])
    if (name === 'books_list' || name === 'writing_tags') return Promise.resolve([])
    const value = extra?.(name, args)
    return Promise.resolve(value)
  })
}

it('设置页显示 MongoDB 文档库未配置状态，保存时按 Tauri camelCase 传参', async () => {
  mockSettings(mongoConfig)
  render(<SettingsPage/>)
  expect(await screen.findByText('尚未配置文档库；配置后可在文档写作页发布文档')).toBeTruthy()
  fireEvent.change(screen.getByLabelText('MongoDB 密码'), { target: { value: 'secret' } })
  fireEvent.click(screen.getByLabelText('保存 MongoDB 文档库配置'))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('mongo_save', { host: '10.0.0.13', port: 27017, user: '文档账号', password: 'secret', authDatabase: 'admin', database: 'personaldesk', collection: 'documents' }))
  expect(await screen.findByText('MongoDB 文档库配置已保存')).toBeTruthy()
})

it('设置页已配置状态显示目标库，测试连接区分手输密码与已保存配置', async () => {
  mockSettings(mongoConfig)
  render(<SettingsPage/>)
  await screen.findByText('尚未配置文档库；配置后可在文档写作页发布文档')
  fireEvent.change(screen.getByLabelText('MongoDB 地址'), { target: { value: '10.0.0.20' } })
  fireEvent.change(screen.getByLabelText('MongoDB 端口'), { target: { value: '27018' } })
  fireEvent.change(screen.getByLabelText('MongoDB 用户名'), { target: { value: 'desk' } })
  fireEvent.change(screen.getByLabelText('MongoDB 密码'), { target: { value: 'secret' } })
  fireEvent.click(screen.getByLabelText('测试 MongoDB 文档库连接'))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('mongo_test', { host: '10.0.0.20', port: 27018, user: 'desk', password: 'secret', authDatabase: 'admin', database: 'personaldesk', collection: 'documents' }))
  expect(await screen.findByText('文档库连接成功')).toBeTruthy()
  fireEvent.change(screen.getByLabelText('MongoDB 密码'), { target: { value: '' } })
  fireEvent.click(screen.getByLabelText('测试 MongoDB 文档库连接'))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('mongo_test_saved'))
})

it('设置页显示已配置的文档库目标，清除前需要确认', async () => {
  const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true)
  mockSettings({ ...mongoConfig, configured: true, host: '10.0.0.9', port: 27019, database: 'desk_docs', collection: 'published' })
  render(<SettingsPage/>)
  expect(await screen.findByText('已配置文档库 10.0.0.9:27019 · desk_docs.published')).toBeTruthy()
  fireEvent.click(screen.getByLabelText('清除 MongoDB 文档库配置'))
  expect(confirm).toHaveBeenCalled()
  expect(invoke.mock.calls.some(call => call[0] === 'mongo_clear')).toBe(false)
  fireEvent.click(screen.getByLabelText('清除 MongoDB 文档库配置'))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('mongo_clear'))
  expect(await screen.findByText('MongoDB 文档库配置已清除')).toBeTruthy()
})

it('文档写作页发布到文档库调用 writing_publish_mongo 并显示 db.collection._id', async () => {
  mockWriting((name) => name === 'writing_publish_mongo' ? 'personaldesk.documents.p' : undefined)
  render(<WritingPage dirty={false} setDirty={() => {}}/>)
  fireEvent.click(await screen.findByText('正文'))
  fireEvent.click(screen.getByRole('button', { name: '文档操作' }))
  fireEvent.click(await screen.findByText('发布到文档库'))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('writing_publish_mongo', { id: 'p' }))
  expect(await screen.findByText('已发布到文档库：personaldesk.documents.p')).toBeTruthy()
})

it('文档有未保存修改时禁用发布到文档库', async () => {
  mockWriting()
  vi.spyOn(window, 'confirm').mockReturnValue(true)
  render(<WritingPage dirty setDirty={() => {}}/>)
  fireEvent.click(await screen.findByText('正文'))
  fireEvent.click(screen.getByRole('button', { name: '文档操作' }))
  const disabledItem = await screen.findByRole('menuitem', { name: '发布到文档库' })
  expect(disabledItem.getAttribute('aria-disabled')).toBe('true')
  fireEvent.click(disabledItem)
  expect(invoke.mock.calls.some(call => call[0] === 'writing_publish_mongo')).toBe(false)
})

it('发布到文档库失败时显示可诊断错误', async () => {
  mockWriting((name) => name === 'writing_publish_mongo' ? Promise.reject(new Error('MongoDB 未配置')) : undefined)
  render(<WritingPage dirty={false} setDirty={() => {}}/>)
  fireEvent.click(await screen.findByText('正文'))
  fireEvent.click(screen.getByRole('button', { name: '文档操作' }))
  fireEvent.click(await screen.findByText('发布到文档库'))
  expect(await screen.findByText('发布到文档库失败：Error: MongoDB 未配置')).toBeTruthy()
})