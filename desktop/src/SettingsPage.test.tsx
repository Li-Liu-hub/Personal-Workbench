// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import SettingsPage from './SettingsPage'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  window.HTMLElement.prototype.scrollIntoView = vi.fn()
})
afterEach(() => { cleanup(); invoke.mockReset() })
it('显示测试数据位置与版本历史', async () => {
  invoke.mockImplementation((name: string) => name === 'settings_info' ? Promise.resolve({ version: '0.15.51', data_directory: 'D:\\测试', releases: [{ version: '0.15.51', title: '更新', changes: ['说明'] }] }) : Promise.reject(new Error('主题读取测试失败')))
  render(<SettingsPage/>)
  fireEvent.click(await screen.findByRole('button', { name: '本地数据' }))
  expect(await screen.findByText('D:\\测试')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '版本历史' }))
  expect(screen.getByText('v0.15.51 · 更新')).toBeTruthy()
})