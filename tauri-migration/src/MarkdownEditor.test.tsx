// @vitest-environment jsdom
import {useState} from 'react'
import {EditorView} from '@codemirror/view'
import {afterEach,beforeAll,expect,it} from 'vitest'
import {cleanup,fireEvent,render,screen} from '@testing-library/react'
import MarkdownEditor from './MarkdownEditor'
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(cleanup)
it('格式工具包围选中文字并可预览 Markdown',()=>{function Example(){const [value,setValue]=useState('文字');return <MarkdownEditor documentId="2cd37b92-5f85-4a6c-8a16-9ca90f6f86ae" label="正文" value={value} onChange={setValue}/>};render(<Example/>);const input=screen.getByRole('textbox',{name:'正文'});const view=EditorView.findFromDOM(input)!;view.dispatch({selection:{anchor:0,head:2}});fireEvent.click(screen.getByRole('button',{name:/粗\s*体/}));expect(view.state.doc.toString()).toBe('**文字**');fireEvent.click(screen.getByText('预览'));expect(screen.getByText('文字').tagName).toBe('STRONG')})
