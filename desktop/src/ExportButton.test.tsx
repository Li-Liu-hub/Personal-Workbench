// @vitest-environment jsdom
import {afterEach,beforeAll,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import ExportButton from './ExportButton'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
beforeAll(()=>{Object.defineProperty(window,'matchMedia',{value:()=>({matches:false,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return false}})});globalThis.ResizeObserver=class{observe(){} unobserve(){} disconnect(){}}})
afterEach(()=>{cleanup();invoke.mockReset();vi.restoreAllMocks()})
it('未保存时取消确认不执行导出，确认后导出选定范围',async()=>{vi.spyOn(window,'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);invoke.mockResolvedValue({path:'D:\\测试.zip',warnings:0});render(<ExportButton scope="documents" dirty/>);fireEvent.click(screen.getByRole('button',{name:/导\s*出/}));const selected=await screen.findByRole('button',{name:'导出文档'});fireEvent.click(selected);expect(invoke).not.toHaveBeenCalled();fireEvent.click(selected);await waitFor(()=>expect(invoke).toHaveBeenCalledWith('export_test_data',{scope:'documents'}));expect(await screen.findByText(/已导出到/)).toBeTruthy()})
