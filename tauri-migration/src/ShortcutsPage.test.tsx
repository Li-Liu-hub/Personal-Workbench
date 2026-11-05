// @vitest-environment jsdom
import {afterEach,beforeAll,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import ShortcutsPage from './ShortcutsPage'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(()=>{cleanup();invoke.mockReset()})
it('打开记录的位置并保留原路径',async()=>{invoke.mockImplementation((name:string)=>Promise.resolve(name==='shortcut_data'?{items:[{id:'a',name:'资料',path:'D:\\资料',is_dir:true,pinned:false}],tags:[],assignments:[],favorites:[],view:'list',undo:false}:undefined));render(<ShortcutsPage/>);fireEvent.click(await screen.findByRole('button',{name:/资料D:/}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('shortcut_open',{path:'D:\\资料'}))})
