// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import App from './App'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches:false, addListener(){}, removeListener(){}, addEventListener(){}, removeEventListener(){}, dispatchEvent(){return false} }) })
  globalThis.ResizeObserver = class { observe(){} unobserve(){} disconnect(){} }
})
afterEach(() => { cleanup(); invoke.mockReset(); vi.restoreAllMocks() })
const entry = { id:'sample', title:'测试服务', username:'测试账号', password:'测试密码', website:'', notes:'测试备注', tags:['含，逗号'] }
function setup(view='grid') {
  invoke.mockImplementation((command: string) => {
    if(command === 'shortcut_data') return Promise.resolve({items:[],tags:[],assignments:[],favorites:[],view:'list',undo:false})
    if(command === 'credentials_list') return Promise.resolve([entry])
    if(command === 'credential_tags') return Promise.resolve(entry.tags)
    if(command === 'credential_favorites') return Promise.resolve([])
    if(command === 'credential_view_get') return Promise.resolve(view)
    return Promise.resolve()
  })
  const result=render(<App />)
  fireEvent.click(screen.getByRole('button',{name:'账号密码管理'}))
  return result
}
describe('账号管理', () => {
  it('同时展示账号、密码和备注，备注复制使用原文', async () => {
    setup()
    expect(await screen.findByText('测试备注')).toBeTruthy()
    expect(screen.getByText('测试账号')).toBeTruthy()
    expect(screen.getByText('测试密码')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', {name:'复制备注'}))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('copy_value', {value:'测试备注'}))
  })
  it('编辑并保存时保留包含逗号的现有分类', async () => {
    setup()
    fireEvent.click(await screen.findByText('测试服务'))
    fireEvent.click(await screen.findByRole('button', {name:'保存账号'}))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('credentials_save', {entry}))
  })
})

it('账号列表保护未保存内容，保存后保留完整分类',async()=>{
 const confirm=vi.spyOn(window,'confirm').mockReturnValue(false)
 setup('list')
 fireEvent.change(await screen.findByLabelText('测试服务备注'),{target:{value:'修改备注'}})
 fireEvent.click(screen.getByRole('button',{name:'物品清单'}))
 expect(confirm).toHaveBeenCalled()
 expect((screen.getByLabelText('测试服务备注') as HTMLTextAreaElement).value).toBe('修改备注')
 fireEvent.click(screen.getByRole('button',{name:'保存行'}))
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('credentials_save',{entry:{...entry,notes:'修改备注'}}))
})
