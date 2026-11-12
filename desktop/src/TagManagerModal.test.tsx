// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import TagManagerModal from './TagManagerModal'
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
})
afterEach(() => { cleanup(); vi.restoreAllMocks() })

it('新增、重命名和删除共用同一套分类管理交互', async () => {
  vi.spyOn(window, 'prompt').mockReturnValue('新分类')
  const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true)
  const onCreate = vi.fn().mockResolvedValue(true)
  const onRename = vi.fn().mockResolvedValue(true)
  const onDelete = vi.fn().mockResolvedValue(true)
  render(<TagManagerModal open tags={['工作']} onClose={() => {}} onCreate={onCreate} onRename={onRename} onDelete={onDelete}/>)
  fireEvent.change(screen.getByLabelText('分类名称'), { target: { value: '新分类' } })
  fireEvent.click(screen.getByRole('button', { name: /添\s*加/ }))
  await waitFor(() => expect(onCreate).toHaveBeenCalledWith('新分类'))
  expect((screen.getByLabelText('分类名称') as HTMLInputElement).value).toBe('')
  fireEvent.click(screen.getByRole('button', { name: '重命名' }))
  await waitFor(() => expect(onRename).toHaveBeenCalledWith('工作', '新分类'))
  fireEvent.click(screen.getByRole('button', { name: /删\s*除/ }))
  expect(confirm).toHaveBeenCalled()
  await waitFor(() => expect(onDelete).toHaveBeenCalledWith('工作'))
})