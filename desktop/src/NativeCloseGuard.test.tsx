// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import NativeCloseGuard from './NativeCloseGuard'
const windowApi = vi.hoisted(() => ({ onCloseRequested: vi.fn(), destroy: vi.fn() }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => windowApi }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
})
afterEach(() => { cleanup(); Reflect.deleteProperty(window, '__TAURI_INTERNALS__'); vi.resetAllMocks() })
it('桌面关闭请求被未保存内容拦截，继续编辑不关闭，确认后才销毁窗口', async () => {
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
  const stop = vi.fn()
  windowApi.onCloseRequested.mockResolvedValue(stop)
  windowApi.destroy.mockResolvedValue(undefined)
  render(<NativeCloseGuard dirty />)
  const preventDefault = vi.fn()
  act(() => windowApi.onCloseRequested.mock.calls[0][0]({ preventDefault }))
  expect(preventDefault).toHaveBeenCalledOnce()
  fireEvent.click(await screen.findByRole('button', { name: '继续编辑' }))
  expect(windowApi.destroy).not.toHaveBeenCalled()
  act(() => windowApi.onCloseRequested.mock.calls[0][0]({ preventDefault }))
  fireEvent.click(await screen.findByRole('button', { name: '放弃修改并关闭' }))
  expect(windowApi.destroy).toHaveBeenCalledOnce()
})
