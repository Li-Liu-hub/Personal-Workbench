// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import InventoryPage from './InventoryPage'
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(()=>{cleanup();invoke.mockReset();vi.restoreAllMocks()})
const item={id:'1',name:'耳机',category:'电子',quantity:2,location:'书桌',brand:'',model:'',notes:'原有备注',tags:['电子'],purchase_date:'',disposal_date:'',purchase_price_cents:1250}
function setup(view='grid'){invoke.mockImplementation((name:string)=>Promise.resolve(name==='inventory_list'?[item]:name==='inventory_tags'?['电子']:name==='inventory_favorites'?[]:name==='inventory_view_get'?view:undefined));render(<InventoryPage/>)}
describe('物品清单',()=>{
 it('编辑时保留未修改字段并以分保存价格',async()=>{setup();fireEvent.click(await screen.findByRole('button',{name:/详\s*情/}));fireEvent.change(screen.getByLabelText('购买价格（元）'),{target:{value:'12.35'}});fireEvent.click(screen.getByRole('button',{name:'保存物品'}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('inventory_save',{entry:{...item,purchase_price_cents:1235}}))})
 it('拒绝超过两位小数的价格',async()=>{setup();fireEvent.click(await screen.findByRole('button',{name:/详\s*情/}));fireEvent.change(screen.getByLabelText('购买价格（元）'),{target:{value:'1.234'}});fireEvent.click(screen.getByRole('button',{name:'保存物品'}));expect(await screen.findAllByText('价格需要为非负金额，最多两位小数')).toHaveLength(2);expect(invoke.mock.calls.some(c=>c[0]==='inventory_save')).toBe(false)})
})

it('列表保存价格、数量并保留未修改的日期和备注',async()=>{setup('list');fireEvent.change(await screen.findByLabelText('耳机价格（元）'),{target:{value:'0.29'}});fireEvent.change(screen.getByLabelText('耳机数量'),{target:{value:'3'}});fireEvent.click(screen.getByRole('button',{name:'保存行'}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('inventory_save',{entry:{...item,quantity:3,purchase_price_cents:29}}))})
it('列表直接编辑品牌和备注',async()=>{setup('list');fireEvent.change(await screen.findByLabelText('耳机品牌'),{target:{value:'索尼'}});fireEvent.change(screen.getByLabelText('耳机备注'),{target:{value:'新备注'}});fireEvent.click(screen.getByRole('button',{name:'保存行'}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('inventory_save',{entry:{...item,brand:'索尼',notes:'新备注'}}))})
it('列表拒绝小数数量并保留输入',async()=>{setup('list');fireEvent.change(await screen.findByLabelText('耳机数量'),{target:{value:'1.5'}});fireEvent.click(screen.getByRole('button',{name:'保存行'}));expect(await screen.findByText(/数量需要填写大于等于 0 的整数/)).toBeTruthy();expect((screen.getByLabelText('耳机数量') as HTMLInputElement).value).toBe('1.5');expect(invoke.mock.calls.some(c=>c[0]==='inventory_save')).toBe(false)})
