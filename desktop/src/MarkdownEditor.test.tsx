// @vitest-environment jsdom
import { EditorView } from '@codemirror/view'
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import MarkdownEditor from './MarkdownEditor'

beforeAll(() => {
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  Object.defineProperty(Range.prototype, 'getClientRects', { value: () => [] })
})
afterEach(cleanup)

it('直接书写 Markdown 标题，回车补全代码块并继续编辑', () => {
  const onChange = vi.fn()
  const text = '# 工作记录\n\n```js'
  render(<MarkdownEditor documentId="doc" label="正文" value={text} onChange={onChange}/>)
  const input = screen.getByRole('textbox', { name: '正文' })
  expect(input.querySelector('.writing-heading-1')?.textContent).toBe('# 工作记录')
  expect(screen.queryByRole('toolbar')).toBeNull()
  const view = EditorView.findFromDOM(input)!
  act(() => view.dispatch({ selection: { anchor: text.length } }))
  fireEvent.keyDown(input, { key: 'Enter', code: 'Enter', keyCode: 13 })
  expect(onChange).toHaveBeenLastCalledWith(text + '\n\n```')
  expect(view.state.selection.main.head).toBe(text.length + 1)
})
