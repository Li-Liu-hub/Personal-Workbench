// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import TodayPage from './TodayPage'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
afterEach(() => { cleanup(); invoke.mockReset() })

it('首页各区独立加载，网站读取失败可重试，随笔从最近编辑进入正确页面', async () => {
  let websiteAttempts = 0
  invoke.mockImplementation(async (command: string) => {
    if (command === 'essay_today') return new Promise(() => {})
    if (command === 'writing_recent') return [
      { id: 'e1', title: '今日记录', updated_at: '2026-11-11T01:00:00Z', is_essay: true },
      { id: 'd1', title: '上周文档', updated_at: '2026-11-04 10:00:00', is_essay: false },
    ]
    if (command === 'websites_list') return [{ id: 'w1', title: '示例网站', url: 'https://example.test' }]
    if (command === 'website_data') {
      if (++websiteAttempts === 1) throw new Error('暂时不可用')
      return { favorites: ['w1'] }
    }
    if (command === 'shortcut_data' || command === 'tool_data') return { items: [], favorites: [] }
    return null
  })
  const navigate = vi.fn(() => true)
  render(<TodayPage navigate={navigate}/>)
  const recent = within(screen.getByRole('region', { name: '最近编辑' }))
  await recent.findByRole('button', { name: /今日记录/ })
  const records = recent.getAllByRole('listitem')
  expect(records[0].textContent).toContain('今日记录')
  fireEvent.click(recent.getByRole('button', { name: '随笔', pressed: false }))
  expect(recent.queryByText('上周文档')).toBeNull()
  fireEvent.click(recent.getByRole('button', { name: /今日记录/ }))
  expect(navigate).toHaveBeenCalledWith('essays', { id: 'e1' })
  expect(invoke).not.toHaveBeenCalledWith('writing_list')
  const websites = within(screen.getByRole('region', { name: '网站收藏' }))
  fireEvent.click(await websites.findByRole('button', { name: '重试' }))
  fireEvent.click(await websites.findByRole('button', { name: /示例网站/ }))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('websites_open', { url: 'https://example.test' }))
})
