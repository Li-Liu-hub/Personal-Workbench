// @vitest-environment jsdom
import { beforeAll, afterEach, describe, it, expect, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import ToolsPage from './ToolsPage'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(()=>{cleanup();invoke.mockReset()})
const data={items:[{id:'a',name:'示例工具',path:'D:\\apps\\tool.exe',notes:'备注'}],favorites:[],tags:['常用'],assignments:[{id:'a',tags:['常用']}],view:'grid'}
describe('小工具启动器',()=>{
 it('显示路径和标签，单击启动，重复点击不启动第二次',async()=>{invoke.mockImplementation((cmd:string)=>Promise.resolve(cmd==='tool_data'?data:undefined));render(<ToolsPage/>);const name=await screen.findByText('示例工具');expect(screen.getByText('D:\\apps\\tool.exe')).toBeTruthy();expect(screen.getByText('常用')).toBeTruthy();fireEvent.click(name);fireEvent.click(name);await waitFor(()=>expect(invoke.mock.calls.filter(([cmd])=>cmd==='tools_launch')).toHaveLength(1))})
 it('选择程序后预填名称和路径',async()=>{invoke.mockImplementation((cmd:string)=>Promise.resolve(cmd==='tool_data'?data:cmd==='tool_pick'?'D:\\apps\\new.exe':undefined));render(<ToolsPage/>);fireEvent.click((await screen.findAllByText('添加程序'))[0]);await waitFor(()=>expect(screen.getByDisplayValue('new')).toBeTruthy());expect(screen.getByDisplayValue('D:\\apps\\new.exe')).toBeTruthy()})
})
