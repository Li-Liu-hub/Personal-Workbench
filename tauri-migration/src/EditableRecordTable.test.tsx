// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import EditableRecordTable from './EditableRecordTable'

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
})
afterEach(() => { cleanup(); vi.restoreAllMocks() })
const items = [{ id: 'one', name: '第一条', tags: ['含，逗号'], notes: '保留原文' }, { id: 'two', name: '第二条', tags: [], notes: '' }]
it('筛选不丢失行编辑，批量保存部分失败只清除已成功的行', async () => {
  const save = vi.fn().mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error('写入失败')).mockResolvedValue(undefined)
  const dirty = vi.fn()
  const props = { fields: [{ key: 'name' as const, title: '名称' }], tags: ['含，逗号'], label: (i: typeof items[number]) => i.name, onSave: save, onDirty: dirty, actions: () => null }
  const result = render(<EditableRecordTable {...props} items={items} />)
  fireEvent.change(screen.getByLabelText('第一条名称'), { target: { value: '修改第一条' } })
  fireEvent.change(screen.getByLabelText('第二条名称'), { target: { value: '修改第二条' } })
  result.rerender(<EditableRecordTable {...props} items={[]} />)
  fireEvent.click(screen.getByRole('button', { name: /保存全部修改/ }))
  expect(await screen.findByText(/未成功保存的输入已保留/)).toBeTruthy()
  expect(save).toHaveBeenNthCalledWith(1, { ...items[0], name: '修改第一条' })
  expect(save).toHaveBeenNthCalledWith(2, { ...items[1], name: '修改第二条' })
  expect(screen.getByText('有 1 行待保存（包含筛选外的行）')).toBeTruthy()
  result.rerender(<EditableRecordTable {...props} items={items} />)
  expect((screen.getByLabelText('第二条名称') as HTMLInputElement).value).toBe('修改第二条')
  fireEvent.click(screen.getByRole('button', { name: /保存全部修改/ }))
  await waitFor(() => expect(dirty).toHaveBeenLastCalledWith(false))
  expect(save).toHaveBeenCalledTimes(3)
})
