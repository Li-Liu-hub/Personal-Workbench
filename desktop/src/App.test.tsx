// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import App from './App'
import type { Theme } from './theme'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  window.HTMLElement.prototype.scrollIntoView = vi.fn()
})
afterEach(() => { cleanup(); invoke.mockReset(); vi.restoreAllMocks() })
const entry = { id: 'sample', title: '测试服务', username: '测试账号', password: '测试密码', website: '', notes: '测试备注', tags: ['含，逗号'] }
const theme: Theme = { accent: [49, 89, 74], text: [37, 42, 38], muted: [108, 116, 108], canvas: [247, 244, 236], sidebar: [27, 45, 37], surface: [255, 253, 248], input: [240, 235, 223], border: [226, 220, 207], button_text: [255, 253, 248], sidebar_text: [201, 211, 202], surface_opacity: 1, sidebar_opacity: 1, image_opacity: 0.7, content_overlay: 0.12, zoom: 1, animations: true, background: null, fit: 'Cover' }
function setup() {
  invoke.mockImplementation((command: string) => {
    if (command === 'theme_get') return Promise.resolve(theme)
    if (command === 'essay_today') return Promise.resolve({ day: '2026-11-11', topic: { id: 'x', category: '生活', title: '今日主题', prompt: '提示' }, page_id: null })
    if (command === 'writing_recent') return Promise.resolve([])
    if (command === 'essay_history') return Promise.resolve([])
    if (command === 'websites_list') return Promise.resolve([])
    if (command === 'website_data') return Promise.resolve({ tags: [], assignments: [], favorites: [], view: 'grid' })
    if (command === 'shortcut_data') return Promise.resolve({ items: [], tags: [], assignments: [], favorites: [], view: 'list', undo: false })
    if (command === 'tool_data') return Promise.resolve({ items: [], favorites: [], tags: [], assignments: [], view: 'grid' })
    if (command === 'credentials_list') return Promise.resolve([entry])
    if (command === 'credential_tags') return Promise.resolve(entry.tags)
    if (command === 'credential_favorites') return Promise.resolve([])
    return Promise.resolve()
  })
  const result = render(<App />)
  fireEvent.click(screen.getByRole('button', { name: '账号密码' }))
  return result
}
describe('账号管理', () => {
  it('直接展示可编辑的账号、密码和备注，备注复制使用原文', async () => {
    setup()
    const notes = await screen.findByLabelText('备注') as HTMLTextAreaElement
    expect(notes.value).toBe('测试备注')
    expect((screen.getByLabelText('账号') as HTMLInputElement).value).toBe('测试账号')
    expect((screen.getByLabelText('密码') as HTMLInputElement).value).toBe('测试密码')
    fireEvent.click(screen.getByRole('button', { name: '复制备注' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('copy_value', { value: '测试备注' }))
  })
  it('无需进入编辑模式即可保存包含逗号的现有分类', async () => {
    setup()
    await screen.findByLabelText('名称')
    fireEvent.click(screen.getByRole('button', { name: '保存账号' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('credentials_save', { entry }))
  })
})

it('账号编辑保护未保存内容，保存后保留完整分类', async () => {
  const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false)
  setup()
  fireEvent.change(await screen.findByLabelText('备注'), { target: { value: '修改备注' } })
  fireEvent.click(screen.getByRole('button', { name: '物品清单' }))
  expect(confirm).toHaveBeenCalled()
  expect((screen.getByLabelText('备注') as HTMLTextAreaElement).value).toBe('修改备注')
  fireEvent.click(screen.getByRole('button', { name: '保存账号' }))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('credentials_save', { entry: { ...entry, notes: '修改备注' } }))
})
