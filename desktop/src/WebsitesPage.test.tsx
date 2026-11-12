// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import WebsitesPage from './WebsitesPage'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  window.HTMLElement.prototype.scrollIntoView = vi.fn()
})
afterEach(() => { cleanup(); invoke.mockReset() })
it('编辑时保留包含分隔符的分类', async () => {
  const entry = { id: '1', title: '测试站', url: 'https://example.test/', category: '工作 · 学习', notes: '' }
  invoke.mockImplementation((name: string) => Promise.resolve(name === 'websites_list' ? [entry] : name === 'website_data' ? { tags: ['工作 · 学习'], assignments: [{ id: '1', tags: ['工作 · 学习'] }], favorites: [], view: 'grid' } : undefined))
  render(<WebsitesPage/>)
  fireEvent.click(await screen.findByRole('button', { name: /打\s*开/ }))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('websites_open', { url: entry.url }))
  fireEvent.click(screen.getByRole('button', { name: /^编\s*辑$/ }))
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('website_save_classified', { entry, tags: ['工作 · 学习'] }))
})

it('网站卡片操作区包含置顶、打开、复制、编辑和删除', async () => {
  const entry = { id: '1', title: '个人网站', url: 'https://example.test/', category: '', notes: '' }
  invoke.mockImplementation((name: string) => Promise.resolve(name === 'websites_list' ? [entry] : name === 'website_data' ? { tags: [], assignments: [], favorites: [], view: 'grid' } : undefined))
  render(<WebsitesPage/>)
  const card = (await screen.findByText('个人网站')).closest('.bookmark-card')
  expect(card).toBeTruthy()
  expect(card?.querySelector('.bookmark-actions')).toBeTruthy()
  expect(card?.querySelectorAll('.bookmark-actions button').length).toBe(5)
})