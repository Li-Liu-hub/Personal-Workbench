import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
type Page = {id:string; book_id:string; title:string; body:string; tags:string[]; revision:number; updated_at:string}
export function Writing({ dirty, setDirty }: {dirty:boolean; setDirty:(value:boolean)=>void}) {
 const [pages,setPages]=useState<Page[]>([]); const [active,setActive]=useState<Page|null>(null); const [error,setError]=useState('')
 async function refresh(){try{setPages(await invoke<Page[]>('writing_list'));setError('')}catch(e){setError(String(e))}}
 useEffect(()=>{void refresh()},[])
 useEffect(()=>{const guard=(event:BeforeUnloadEvent)=>{if(dirty){event.preventDefault();event.returnValue=''}}; window.addEventListener('beforeunload',guard);return()=>window.removeEventListener('beforeunload',guard)},[dirty])
 function canLeave(){return !dirty || window.confirm('当前文档有未保存的修改，确定离开吗？')}
 async function create(){if(!canLeave()) return; const title=window.prompt('文档名称'); if(!title?.trim())return; try{const page=await invoke<Page>('writing_create',{title,bookId:null});setActive(page);setDirty(false);await refresh()}catch(e){setError(String(e))}}
 async function save(){if(!active)return;try{const page=await invoke<Page>('writing_save',{page:active});setActive(page);setDirty(false);await refresh()}catch(e){setError(String(e))}}
 return <><header><h1>文档写作</h1><button onClick={()=>void create()}>新建文档</button></header>{error && <p className="error" role="alert">{error}</p>}<div className="writing"><aside>{pages.map(page=><button key={page.id} onClick={()=>{if(canLeave()){setActive(page);setDirty(false)}}}>{page.title}</button>)}</aside>{active && <section className="editor"><label>标题<input value={active.title} onChange={e=>{setActive({...active,title:e.target.value});setDirty(true)}}/></label><label>正文<textarea rows={20} value={active.body} onChange={e=>{setActive({...active,body:e.target.value});setDirty(true)}}/></label><button disabled={!dirty} onClick={()=>void save()}>保存</button>{dirty && <span> 有未保存的修改</span>}</section>}</div></>
}
