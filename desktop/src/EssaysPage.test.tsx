// @vitest-environment jsdom
import {EditorView} from '@codemirror/view'
import {afterEach,beforeAll,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen} from '@testing-library/react'
import EssaysPage from './EssaysPage'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(()=>{cleanup();invoke.mockReset()})
it('开始写作后保存编辑内容',async()=>{const topic={day:'2026-11-05',topic:{id:'x',category:'生活',title:'今日主题',prompt:'提示'},page_id:null};const page={id:'1',book_id:'root',title:'标题',body:'',tags:[],revision:1,updated_at:''};invoke.mockImplementation((name:string,args:any)=>Promise.resolve(name==='essay_today'?topic:name==='essay_start'?page:name==='writing_save'?args.page:undefined));const setDirty=vi.fn();render(<EssaysPage dirty={false} setDirty={setDirty}/>);fireEvent.click(await screen.findByRole('button',{name:'开始写作'}));EditorView.findFromDOM(await screen.findByRole('textbox',{name:'正文'}))!.dispatch({changes:{from:0,insert:'内容'}});expect(setDirty).toHaveBeenCalledWith(true)})
