// @vitest-environment jsdom
import {afterEach,beforeAll,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import WebsitesPage from './WebsitesPage'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(()=>{cleanup();invoke.mockReset()})
it('编辑时保留包含分隔符的分类',async()=>{const entry={id:'1',title:'测试站',url:'https://example.test/',category:'工作 · 学习',notes:''};invoke.mockImplementation((name:string)=>Promise.resolve(name==='websites_list'?[entry]:name==='website_data'?{tags:['工作 · 学习'],assignments:[{id:'1',tags:['工作 · 学习']}],favorites:[]}:undefined));render(<WebsitesPage/>);fireEvent.click(await screen.findByRole('button',{name:'打开网站'}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('websites_open',{url:entry.url}));fireEvent.click(screen.getByRole('button',{name:/编\s*辑/}));fireEvent.click(screen.getByRole('button',{name:/保\s*存/}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('website_save_classified',{entry,tags:['工作 · 学习']}))})


it('切换列表视图后按原设置名称保存',async()=>{let view='grid';invoke.mockImplementation((name:string,args?:{mode?:string})=>{if(name==='website_view'){view=args?.mode||'grid';return Promise.resolve()}return Promise.resolve(name==='websites_list'?[]:name==='website_data'?{tags:[],assignments:[],favorites:[],view}:undefined)});render(<WebsitesPage/>);await screen.findByText('网站列表 · 0');fireEvent.click(screen.getByText('列表'));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('website_view',{mode:'list'}))})
it('网站操作按钮放在可换行的独立操作区',async()=>{const entry={id:'1',title:'个人网站',url:'https://example.test/',category:'',notes:''};invoke.mockImplementation((name:string)=>Promise.resolve(name==='websites_list'?[entry]:name==='website_data'?{tags:[],assignments:[],favorites:[],view:'grid'}:undefined));render(<WebsitesPage/>);const card=(await screen.findByText('个人网站')).closest('.website-card');expect(card).toBeTruthy();expect(card?.querySelector('.website-actions')).toBeTruthy();expect(card?.querySelectorAll('.website-actions button').length).toBe(5)})
