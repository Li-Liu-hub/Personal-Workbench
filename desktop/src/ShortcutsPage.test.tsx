// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import ShortcutsPage from './ShortcutsPage'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  window.HTMLElement.prototype.scrollIntoView = vi.fn()
})
afterEach(() => { cleanup(); invoke.mockReset() })
it('打开记录的位置并保留原路径', async () => {
  invoke.mockImplementation((name: string) => Promise.resolve(name === 'shortcut_data' ? { items: [{ id: 'a', name: '资料', path: 'D:\\资料', is_dir: true, pinned: false }], tags: [], assignments: [], favorites: [], view: 'list', undo: false } : undefined))
  render(<ShortcutsPage/>)
  const title = await screen.findByText('资料')
  fireEvent.click(title.closest('button')!)
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('shortcut_open', { path: 'D:\\资料' }))
})
it('名称和路径始终显示在独立标题区', async () => {
  invoke.mockImplementation((name: string) => Promise.resolve(name === 'shortcut_data' ? { items: [{ id: 'a', name: '项目资料', path: 'D:\\长文件夹\\项目资料', is_dir: true, pinned: false }], tags: [], assignments: [], favorites: [], view: 'grid', undo: false } : undefined))
  render(<ShortcutsPage/>)
  expect(await screen.findByText('项目资料')).toBeTruthy()
  expect(screen.getByText('D:\\长文件夹\\项目资料')).toBeTruthy()
  expect(document.querySelector('.shortcut-row .shortcut-copy')).toBeTruthy()
})