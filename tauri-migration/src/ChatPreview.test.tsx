// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import ChatPreview from './ChatPreview'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
})
afterEach(() => { cleanup(); invoke.mockReset() })
it('完整显示第一行和不等长单元格，切换工作表后复制完整原文', async () => {
  invoke.mockResolvedValue(undefined)
  const chat = { id: 'chat', title: '聊天.xlsx', format: 'xlsx', text: '完整\t原文\n下一行', sheets: [{ name: '工作表一', rows: [['001'], ['第二行', '右侧单元格']] }, { name: '工作表二', rows: [['另一个工作表']] }] }
  const exported = vi.fn()
  render(<ChatPreview chat={chat} onExport={exported} />)
  expect(screen.getByText('001')).toBeTruthy()
  expect(screen.getByText('右侧单元格')).toBeTruthy()
  fireEvent.click(screen.getByRole('tab', { name: '工作表二' }))
  expect(screen.getByText('另一个工作表')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '复制全部文字' }))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('copy_value', { value: chat.text }))
  fireEvent.click(screen.getByRole('button', { name: '导出原件' }))
  expect(exported).toHaveBeenCalledOnce()
})
