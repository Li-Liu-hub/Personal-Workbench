import useAutoSave from './useAutoSave'
import WritingZoom from './WritingZoom'
import ExportButton from './ExportButton'
import MarkdownEditor from './MarkdownEditor'
import {useWritingDraft} from './useWritingDraft'
import {useEffect,useRef,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {Alert,Button,Card,Empty,Input,Segmented,Space} from 'antd'
type Assignment={day:string;topic:{id:string;category:string;title:string;prompt:string};page_id:string|null}
type Page={id:string;book_id:string;title:string;body:string;tags:string[];revision:number;updated_at:string}
type Summary={day:string;title:string;topic:string;page_id:string}
export default function EssaysPage({dirty,setDirty}:{dirty:boolean;setDirty:(v:boolean)=>void}){
 const [today,setToday]=useState<Assignment|null>(null),[history,setHistory]=useState<Summary[]>([]),[query,setQuery]=useState(''),[mode,setMode]=useState<'today'|'history'>('today'),[page,setPage]=useState<Page|null>(null),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 const mounted=useRef(true)
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false}},[])
 const {flush,draftError}=useWritingDraft(page,dirty)
 async function loadToday(){try{setToday(await invoke<Assignment>('essay_today'));setError('')}catch(e){setError(`读取今日主题失败：${String(e)}`)}}
 async function loadHistory(q:string){try{setHistory(await invoke<Summary[]>('essay_history',{query:q}));setError('')}catch(e){setError(`读取记录失败：${String(e)}`)}}
 useEffect(()=>{void loadToday()},[])
 useEffect(()=>{if(mode==='history')void loadHistory(query)},[mode,query])
 useEffect(()=>{const guard=(e:BeforeUnloadEvent)=>{if(dirty){e.preventDefault();e.returnValue=''}};window.addEventListener('beforeunload',guard);return()=>window.removeEventListener('beforeunload',guard)},[dirty])
 function leave(){if(busy)return;if(dirty&&!window.confirm('随笔有未保存的修改，确定离开吗？'))return;setPage(null);setDirty(false);void loadToday()}
 async function start(){if(!today)return;setBusy(true);try{setPage(await invoke<Page>('essay_start',{day:today.day}));setDirty(false)}catch(e){setError(`打开随笔失败：${String(e)}`)}finally{setBusy(false)}}
 async function open(id:string){try{const p=await invoke<Page|null>('essay_page',{id});if(!p)throw new Error('随笔已不存在');setPage(p);setDirty(false)}catch(e){setError(`打开随笔失败：${String(e)}`)}}
 async function save(){if(!page||busy)return;setBusy(true);try{await flush(page);const saved=await invoke<Page>('writing_save',{page});if(mounted.current){setPage(saved);setDirty(false);setError('')}}catch(e){if(mounted.current)setError(`保存失败：${String(e)}`)}finally{if(mounted.current)setBusy(false)}}
 useAutoSave(page,dirty,busy,save)
 useEffect(()=>{const handler=(event:KeyboardEvent)=>{if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'&&page){event.preventDefault();if(dirty&&!busy)void save()}};window.addEventListener('keydown',handler);return()=>window.removeEventListener('keydown',handler)},[page,dirty,busy])
 async function reroll(){if(!today)return;try{setToday(await invoke<Assignment>('essay_reroll',{day:today.day,expected:today.topic.id}))}catch(e){setError(`更换主题失败：${String(e)}`)}}
 return <div className="content"><header className="page-header"><div><div className="eyebrow">DAILY WRITING</div><h1>每日随笔</h1><p>记录想法与生活</p></div>{page?<Space><WritingZoom/><span>{Array.from(page.body).length} 字</span><ExportButton scope="essays" dirty={dirty}/><Button onClick={leave}>返回</Button><Button type="primary" loading={busy} disabled={!dirty} onClick={()=>void save()}>保存</Button></Space>:<Space><ExportButton scope="essays"/><Segmented value={mode} onChange={v=>setMode(v as 'today'|'history')} options={[{value:'today',label:'今日主题'},{value:'history',label:'记录本'}]}/></Space>}</header>{error&&<Alert type="error" showIcon closable message={error} onClose={()=>setError('')}/>} {draftError&&<Alert type="error" showIcon message={draftError}/>} {page?<article className="essay-paper"><Input className="document-title" variant="borderless" aria-label="标题" disabled={busy} value={page.title} onChange={e=>{setPage({...page,title:e.target.value});setDirty(true)}}/><MarkdownEditor key={page.id} label="正文" disabled={busy} rows={20} value={page.body} onChange={body=>{setPage({...page,body});setDirty(true)}}/><div className="document-status"><span>{dirty?'等待自动保存':'已保存'}</span><span>Ctrl+V 粘贴图片</span></div></article>:mode==='today'?today&&<Card><p>{today.day} / {today.topic.category}</p><h2>{today.topic.title}</h2><p>{today.topic.prompt}</p><Space><Button type="primary" loading={busy} onClick={()=>void start()}>{today.page_id?'继续写作':'开始写作'}</Button>{!today.page_id&&<Button onClick={()=>void reroll()}>换个主题</Button>}</Space></Card>:<><Input aria-label="搜索随笔" placeholder="搜索日期、主题、标题或正文" value={query} onChange={e=>setQuery(e.target.value)} allowClear/>{history.length?history.map(row=><Card key={row.page_id} size="small" hoverable onClick={()=>void open(row.page_id)}><strong>{row.title}</strong><p>{row.day} · {row.topic}</p></Card>):<Empty description="暂无随笔"/>}</>}</div>
}
