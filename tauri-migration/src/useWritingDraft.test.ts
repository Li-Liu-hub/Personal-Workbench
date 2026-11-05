// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest'
import {act,cleanup,renderHook} from '@testing-library/react'
import {useWritingDraft} from './useWritingDraft'
const invoke=vi.hoisted(()=>vi.fn())
vi.mock('@tauri-apps/api/core',()=>({invoke}))
afterEach(()=>{cleanup();vi.useRealTimers();invoke.mockReset()})
it('依次写入草稿并在正式保存前等待最新内容',async()=>{vi.useFakeTimers();let release!:()=>void;invoke.mockImplementationOnce(()=>new Promise<void>(r=>{release=r})).mockResolvedValue(undefined);const base={id:'p',book_id:'b',title:'文档',body:'一',tags:[],revision:1,updated_at:''};const {result,rerender}=renderHook(({page})=>useWritingDraft(page,true),{initialProps:{page:base}});await act(async()=>{vi.advanceTimersByTime(800);await Promise.resolve()});expect(invoke).toHaveBeenCalledTimes(1);const latest={...base,body:'二'};rerender({page:latest});let done=false;let flush!:Promise<void>;await act(async()=>{flush=result.current.flush(latest).then(()=>{done=true});await Promise.resolve()});expect(done).toBe(false);await act(async()=>{release();await flush});expect(invoke).toHaveBeenNthCalledWith(2,'writing_draft',{page:latest});expect(done).toBe(true)})
