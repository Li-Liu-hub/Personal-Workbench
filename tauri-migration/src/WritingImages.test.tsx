// @vitest-environment jsdom
import {useState} from 'react'
import {EditorView} from '@codemirror/view'
import {afterEach,beforeAll,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import MarkdownEditor from './MarkdownEditor'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}};Object.defineProperty(Range.prototype,'getClientRects',{value:()=>[]});Object.defineProperty(Range.prototype,'getBoundingClientRect',{value:()=>({left:0,right:0,top:0,bottom:0,width:0,height:0})})})
afterEach(()=>{cleanup();invoke.mockReset()})
it('粘贴后直接显示图片、继续输入并保持 Markdown',async()=>{invoke.mockImplementation((name:string)=>Promise.resolve(name==='writing_image_paste'?'writing-assets/pasted.png':'data:image/png;base64,AAAA'));let value='前后';function Example(){const [v,setValue]=useState(value);return <MarkdownEditor label="正文" value={v} onChange={v=>{value=v;setValue(v)}}/>};render(<Example/>);const input=screen.getByRole('textbox',{name:'正文'});const view=EditorView.findFromDOM(input)!;view.dispatch({selection:{anchor:1}});const file={size:3,arrayBuffer:async()=>new Uint8Array([1,2,3]).buffer};fireEvent.paste(input,{clipboardData:{items:[{kind:'file',type:'image/png',getAsFile:()=>file}]}});await waitFor(()=>expect(value).toBe('前![图片](writing-assets/pasted.png)后'));const img=await screen.findByRole('img',{name:'图片'});await waitFor(()=>expect(img.getAttribute('src')).toBe('data:image/png;base64,AAAA'));view.dispatch({changes:{from:view.state.selection.main.head,insert:'继续'}});expect(value).toBe('前![图片](writing-assets/pasted.png)继续后')})
it('重新打开时直接渲染已保存的图片引用',async()=>{invoke.mockResolvedValue('data:image/png;base64,AAAA');render(<MarkdownEditor label="正文" value="![图片](writing-assets/pasted.png)" onChange={()=>{}}/>);const img=await screen.findByRole('img',{name:'图片'});await waitFor(()=>expect(img.getAttribute('src')).toBe('data:image/png;base64,AAAA'));expect(invoke).toHaveBeenCalledWith('writing_image_read',{reference:'writing-assets/pasted.png'})})
