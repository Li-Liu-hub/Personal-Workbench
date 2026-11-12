// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import CommandPalette from './CommandPalette'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  window.HTMLElement.prototype.scrollIntoView = vi.fn()
})
afterEach(() => { cleanup(); invoke.mockReset() })
it('跨模块搜索并跳转到账号详情', async () => {
  invoke.mockImplementation((name: string) => Promise.resolve(
    name === 'writing_search' ? { hits: [], total: 0 }
      : name === 'contact_search_items' ? []
          : name === 'credential_search_items' ? [{ id: 'c1', title: '测试服务', username: 'desk', website: '', tags: [] }]
            : name === 'websites_list' ? []
              : name === 'shortcut_data' ? { items: [] }
                : name === 'tool_data' ? { items: [] }
                  : name === 'inventory_list' ? []
                    : undefined,
  ))
  const onNavigate = vi.fn()
  render(<CommandPalette open onClose={() => {}} onNavigate={onNavigate}/>)
  const input = screen.getByLabelText('搜索全部内容')
  fireEvent.change(input, { target: { value: '测试服务' } })
  const result = await screen.findByText('测试服务')
  expect(result).toBeTruthy()
  fireEvent.keyDown(input, { key: 'Enter' })
  await waitFor(() => expect(onNavigate).toHaveBeenCalledWith('credentials', { id: 'c1' }))
})
it('空查询展示模块跳转', async () => {
  invoke.mockResolvedValue([])
  render(<CommandPalette open onClose={() => {}} onNavigate={() => true}/>)
  expect(await screen.findByText('文档写作')).toBeTruthy()
  expect(screen.getByText('每日随笔')).toBeTruthy()
  expect(invoke).not.toHaveBeenCalled()
})

it('标题优先，切换范围后可用方向键选择，输入法确认不打开结果', async () => {
  const fixtures: Record<string, unknown> = {
    writing_search: { total: 2, hits: [
      { id: 'p1', title: '工作计划', excerpt: '其中包括测试服务', is_essay: false, score: 165 },
      { id: 'p2', title: '发布安排', excerpt: '上线测试服务', is_essay: false, score: 165 },
    ] },
    websites_list: [{ id: 'w1', title: '测试服务', url: 'https://example.test', notes: '' }],
    contact_search_items: [],
    shortcut_data: { items: [] },
    tool_data: { items: [] },
  }
  invoke.mockImplementation((name: string) => Promise.resolve(fixtures[name] ?? []))
  const onNavigate = vi.fn(() => true)
  const onClose = vi.fn()
  render(<CommandPalette open onClose={onClose} onNavigate={onNavigate}/>)
  const input = screen.getByRole('combobox')
  fireEvent.change(input, { target: { value: '测试服务' } })
  await screen.findByText('3 条结果')
  expect(screen.getByRole('option', { selected: true }).textContent).toContain('example.test')
  fireEvent.click(screen.getByRole('button', { name: '文档' }))
  await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(2))
  fireEvent.keyDown(input, { key: 'ArrowDown' })
  expect(screen.getByRole('option', { selected: true }).textContent).toContain('发布安排')
  fireEvent.keyDown(input, { key: 'Enter', isComposing: true })
  expect(onNavigate).not.toHaveBeenCalled()
  fireEvent.keyDown(input, { key: 'Enter' })
  expect(onNavigate).toHaveBeenCalledWith('writing', { id: 'p2' })
  await waitFor(() => expect(onClose).toHaveBeenCalledWith('navigate'))
})

it('先选文档范围时只请求写作摘要', async () => {
  invoke.mockResolvedValue({ hits: [], total: 0 })
  render(<CommandPalette open onClose={() => {}} onNavigate={() => true}/>)
  fireEvent.click(screen.getByRole('button', { name: '文档' }))
  fireEvent.change(screen.getByRole('combobox'), { target: { value: '报告' } })
  await screen.findByText('0 条结果')
  expect(invoke.mock.calls).toEqual([['writing_search', { query: '报告', scope: 'writing' }]])
})
