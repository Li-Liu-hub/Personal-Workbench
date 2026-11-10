// @vitest-environment jsdom
import {afterEach,beforeAll,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import ThemeEditor from './ThemeEditor'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(()=>{cleanup();invoke.mockReset()})
it('修改颜色时保留背景和透明度设置',async()=>{const color=[1,2,3];const theme={accent:color,text:color,muted:color,canvas:color,sidebar:color,surface:color,input:color,border:color,button_text:color,sidebar_text:color,surface_opacity:0.5,sidebar_opacity:0.8,image_opacity:0.7,content_overlay:0.1,zoom:1,animations:true,background:'images/example.png',fit:'Cover'};invoke.mockImplementation((name:string,args?:{theme:typeof theme})=>Promise.resolve(name==='theme_get'?theme:name==='theme_save'?args?.theme:'data:image/png;base64,AA=='));render(<ThemeEditor/>);fireEvent.change(await screen.findByLabelText('主题色'),{target:{value:'#abcdef'}});fireEvent.click(screen.getByRole('button',{name:'保存主题'}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('theme_save',{theme:{...theme,accent:[171,205,239]}}))})
