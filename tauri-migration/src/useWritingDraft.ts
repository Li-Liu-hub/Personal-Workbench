import {useCallback,useEffect,useRef,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
type Page={id:string;book_id:string;title:string;body:string;tags:string[];revision:number;updated_at:string}
/** Queue draft writes so an older edit cannot finish after a newer save. */
export function useWritingDraft(page:Page|null,dirty:boolean){
 const timer=useRef<ReturnType<typeof setTimeout>|null>(null)
 const pending=useRef<Promise<void>>(Promise.resolve())
 const [draftError,setDraftError]=useState('')
 const enqueue=useCallback((value:Page)=>{
  const write=pending.current.catch(()=>{}).then(()=>invoke('writing_draft',{page:value})).then(()=>{setDraftError('')})
  pending.current=write
  return write
 },[])
 useEffect(()=>{
  if(!page||!dirty)return
  timer.current=setTimeout(()=>{timer.current=null;void enqueue(page).catch(e=>setDraftError(`自动保存草稿失败：${String(e)}`))},800)
  return ()=>{if(timer.current){clearTimeout(timer.current);timer.current=null}}
 },[page,dirty,enqueue])
 const flush=useCallback(async(value:Page)=>{if(timer.current){clearTimeout(timer.current);timer.current=null}await enqueue(value)},[enqueue])
 return {flush,draftError}
}
