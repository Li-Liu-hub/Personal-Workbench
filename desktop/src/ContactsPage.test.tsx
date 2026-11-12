// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import ContactsPage from './ContactsPage'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  window.HTMLElement.prototype.scrollIntoView = vi.fn()
})
afterEach(() => { cleanup(); invoke.mockReset() })
const base = { id: '1', name: '示例', school: '', major: '', work: '', phone: '', national_id: '', address: '', gender: '', birthday: '', hobbies: '', chat_notes: '', chats: [] }

it('显示联系人照片并在重新导入后刷新', async () => {
  let photos = 0
  invoke.mockImplementation((name: string) => Promise.resolve(
    name === 'contact_data' ? { items: [base], tags: [], assignments: [], favorites: [] }
      : name === 'contact_photo' ? `data:image/png;base64,${++photos}`
        : name === 'contact_import_photo' ? true
          : undefined,
  ))
  render(<ContactsPage/>)
  expect((await screen.findByAltText('示例的照片')).getAttribute('src')).toContain('base64,1')
  fireEvent.click(screen.getByRole('button', { name: '导入照片' }))
  await waitFor(() => expect(screen.getByAltText('示例的照片').getAttribute('src')).toContain('base64,2'))
})

it('详情页直接展示完整资料，不再依赖列表附加列开关', async () => {
  const item = { ...base, national_id: '123' }
  invoke.mockImplementation((name: string) => Promise.resolve(
    name === 'contact_data' ? { items: [item], tags: [], assignments: [], favorites: [], view: 'grid', extra: false } : undefined,
  ))
  render(<ContactsPage/>)
  expect(await screen.findByText('123')).toBeTruthy()
  expect(screen.getByRole('heading', { name: '示例' })).toBeTruthy()
})

it('详情编辑只保存目标资料并保留聊天备注', async () => {
  const item = { ...base, phone: '原号码', chat_notes: '原备注' }
  invoke.mockImplementation((name: string) => Promise.resolve(
    name === 'contact_data' ? { items: [item], tags: ['朋友'], assignments: [{ id: '1', tags: ['朋友'] }], favorites: [], view: 'list', extra: false } : undefined,
  ))
  const dirty = vi.fn()
  render(<ContactsPage setWorkspaceDirty={dirty}/>)
  fireEvent.click(await screen.findByRole('button', { name: '编辑联系人' }))
  fireEvent.change(screen.getByLabelText('电话'), { target: { value: '新号码' } })
  expect(dirty).toHaveBeenCalledWith(true)
  fireEvent.click(screen.getByRole('button', { name: /保\s*存/ }))
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('contact_save_classified', { entry: { ...item, phone: '新号码' }, tags: ['朋友'] }))
  expect(dirty).toHaveBeenLastCalledWith(false)
})