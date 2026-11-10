import useAutoSave from './useAutoSave'
import WritingZoom from './WritingZoom'
import MarkdownEditor from './MarkdownEditor'
import {useWritingDraft} from './useWritingDraft'
import {useEffect,useRef,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {Alert,Button,Dropdown,Input,Modal,Select,Space,Tree,Tooltip} from 'antd'
import { CheckOutlined, FileTextOutlined, FolderOutlined, MoreOutlined, PlusOutlined, SearchOutlined, MenuFoldOutlined } from '@ant-design/icons'
import type {DataNode} from 'antd/es/tree'
type Page={id:string;book_id:string;title:string;body:string;tags:string[];revision:number;updated_at:string}
type Book={id:string;parent_id:string|null;title:string;tags:string[]}
export default function WritingPage({dirty,setDirty}:{dirty:boolean;setDirty:(v:boolean)=>void}){
 const [pages,setPages]=useState<Page[]>([]),[books,setBooks]=useState<Book[]>([]),[active,setActive]=useState<Page|null>(null),[selectedBook,setSelectedBook]=useState<string|null>(null),[error,setError]=useState(''),[notice,setNotice]=useState(''),[moving,setMoving]=useState<{kind:'book'|'page';id:string;title:string}|null>(null),[destination,setDestination]=useState(''),[query,setQuery]=useState(''),[tags,setTags]=useState<string[]>([]),[manageTags,setManageTags]=useState(false),[newTag,setNewTag]=useState(''),[busy,setBusy]=useState(false)
 const [naming,setNaming]=useState<{book:Book|null}|null>(null),[name,setName]=useState(''),[removing,setRemoving]=useState<{id:string;page:boolean}|null>(null),[recoverOpen,setRecoverOpen]=useState(false),[directoryOpen,setDirectoryOpen]=useState(true)
 const mounted=useRef(true)
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false}},[])
 const {flush,draftError}=useWritingDraft(active,dirty)
 async function refresh(){try{const [p,b,t]=await Promise.all([invoke<Page[]>('writing_list'),invoke<Book[]>('books_list'),invoke<string[]>('writing_tags')]);setPages(p);setBooks(b);setTags(t);setError('')}catch(e){setError(`读取文档失败：${String(e)}`)}}
 useEffect(()=>{void refresh()},[])
 useEffect(()=>{const guard=(e:BeforeUnloadEvent)=>{if(dirty){e.preventDefault();e.returnValue=''}};window.addEventListener('beforeunload',guard);return()=>window.removeEventListener('beforeunload',guard)},[dirty])
 function canLeave(){if(busy)return false;return !dirty||window.confirm('文档有未保存的修改，确定离开吗？')}
 async function save(){if(!active||busy)return;setBusy(true);try{await flush(active);const saved=await invoke<Page>('writing_save',{page:active});if(mounted.current){setActive(saved);setDirty(false);await refresh()}}catch(e){if(mounted.current)setError(`保存失败：${String(e)}`)}finally{if(mounted.current)setBusy(false)}}
 useAutoSave(active,dirty,busy,save)
 useEffect(()=>{const handler=(event:KeyboardEvent)=>{if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='s'&&active){event.preventDefault();if(dirty&&!busy)void save()}};window.addEventListener('keydown',handler);return()=>window.removeEventListener('keydown',handler)},[active,dirty,busy])
 async function draft(){if(!active)return;setBusy(true);try{await flush(active);setNotice('草稿已保存；正式文档尚未更新');setError('')}catch(e){setError(`保存草稿失败：${String(e)}`)}finally{setBusy(false)}}
 async function recover(){if(!canLeave())return;setBusy(true);try{await invoke('writing_recover_drafts');await refresh();setNotice('已检查并恢复草稿；恢复内容会作为独立文档保留')}catch(e){setError(`恢复草稿失败：${String(e)}`)}finally{setBusy(false)}}
 async function exportPage(){if(!active)return;setBusy(true);try{const path=await invoke<string|null>('writing_export_markdown',{page:active});if(path)setNotice(`文档已导出：${path}。如含图片，请保留旁边的图片文件夹。`)}catch(e){setError(`导出失败：${String(e)}`)}finally{setBusy(false)}}
 async function publishBlog(){if(!active)return;if(dirty){setError('请先保存当前文档再发布到博客');return}setBusy(true);try{const url=await invoke<string>('writing_publish_blog',{page:active});setNotice(`已发布到博客：${url}`);setError('')}catch(e){setError(`发布博客失败：${String(e)}`)}finally{setBusy(false)}}
 async function publishMongo(){if(!active)return;if(dirty){setError('请先保存当前文档，再发布到文档库');return}setBusy(true);try{const target=await invoke<string>('writing_publish_mongo',{id:active.id});setNotice(`已发布到文档库：${target}`);setError('')}catch(e){setError(`发布到文档库失败：${String(e)}`)}finally{setBusy(false)}}
 async function createPage(){if(!canLeave())return;const title='未命名文档';try{const p=await invoke<Page>('writing_create',{title,bookId:selectedBook});setActive(p);setDirty(false);await refresh()}catch(e){setError(`新建文档失败：${String(e)}`)}}
 async function saveCopy(){if(!active)return;setBusy(true);try{await flush(active);const copy=await invoke<Page>('writing_save_copy',{page:active});setActive(copy);setDirty(false);await refresh();setNotice('已保存为新文档，原文档未覆盖')}catch(e){setError(`另存副本失败：${String(e)}`)}finally{setBusy(false)}}
 async function changeTag(command:string,args:Record<string,string>){if(dirty){setError('请先保存当前文档再管理标签');return false}try{await invoke(command,args);await refresh();if(active){const updated=await invoke<Page|null>('essay_page',{id:active.id});if(updated)setActive(updated)}return true}catch(e){setError(`管理标签失败：${String(e)}`);return false}}
 async function createLog(){if(!canLeave())return;setBusy(true);try{const p=await invoke<Page>('writing_daily_log',{bookId:selectedBook});setActive(p);setDirty(false);await refresh()}catch(e){setError(`打开日志失败：${String(e)}`)}finally{setBusy(false)}}
 function createBook(){setName('');setNaming({book:null})}
 function renameBook(book:Book){setName(book.title);setNaming({book})}
 async function saveBookName(){
  if(!name.trim()||busy)return
  setBusy(true)
  try{await invoke('books_save',{book:naming?.book?{...naming.book,title:name.trim()}:{id:crypto.randomUUID(),title:name.trim(),parent_id:selectedBook,tags:[]}});setNaming(null);await refresh()}
  catch(e){setError(`保存图书失败：${String(e)}`)}finally{setBusy(false)}
 }
 async function remove(id:string,page:boolean){if(!canLeave())return;try{await invoke('writing_delete',{id,page});setRemoving(null);setActive(null);setDirty(false);setSelectedBook(null);await refresh()}catch(e){setError(`删除失败：${String(e)}`)}}
 function chooseMove(kind:'book'|'page',id:string,title:string){if(dirty){setError('请先保存当前文档，再移动目录');return}setDestination('');setMoving({kind,id,title})}
 async function move(){if(!moving)return;setBusy(true);try{if(moving.kind==='book')await invoke('writing_move_book',{id:moving.id,parent:destination||null});else await invoke('writing_move_page',{id:moving.id,target:destination||null});setMoving(null);setActive(null);setSelectedBook(null);await refresh();setNotice('移动完成')}catch(e){setError(`移动失败：${String(e)}`)}finally{setBusy(false)}}
 const term=query.trim().toLowerCase()
 function pageNodes(book:string):DataNode[]{return pages.filter(p=>p.book_id===book&&(!term||[p.title,p.body,...p.tags].some(v=>v.toLowerCase().includes(term)))).map(p=>({key:`page:${p.id}`,icon:<FileTextOutlined/>,title:<span className="document-tree-name" title={p.title}>{p.title}</span>,isLeaf:true}))}
 function bookNodes(parent:string|null):DataNode[]{return books.filter(b=>(b.parent_id||null)===parent).flatMap(b=>{const children=[...bookNodes(b.id),...pageNodes(b.id)];if(term&&!b.title.toLowerCase().includes(term)&&!children.length)return [];return [{key:`book:${b.id}`,icon:<FolderOutlined/>,title:<div className="document-tree-folder"><span className="document-tree-name" title={b.title}>{b.title}</span><span onClick={e=>e.stopPropagation()}><Dropdown trigger={['click']} menu={{items:[{key:'rename',label:'重命名'},{key:'move',label:'移动图书'},{key:'delete',label:'删除图书',danger:true}],onClick:({key})=>{if(key==='rename')renameBook(b);else if(key==='move')chooseMove('book',b.id,b.title);else setRemoving({id:b.id,page:false})}}}><Button type="text" size="small" className="tree-more" aria-label={`${b.title}操作`} icon={<MoreOutlined/>}/></Dropdown></span></div>,children}]})}
 const nodes:DataNode[]=[{key:'root',title:'全部文档',icon:<FolderOutlined/>,children:[...bookNodes(null),...pageNodes('__personal_writing__')]}]
 return <div className={`content writing-page ${directoryOpen?'':'directory-collapsed'}`}>
  <header className="writing-header">
   <div className="writing-header-title"><Tooltip title={directoryOpen?'收起目录':'展开目录'}><Button type="text" aria-label="切换文档目录" icon={<MenuFoldOutlined/>} onClick={()=>setDirectoryOpen(v=>!v)}/></Tooltip><h1>文档写作</h1><span className="writing-total">{pages.length} 篇</span></div>
   <Space><Dropdown trigger={['click']} menu={{items:[{key:'log',label:'生成今日日志'},{key:'recover',label:'恢复草稿'}],onClick:({key})=>key==='log'?void createLog():setRecoverOpen(true)}}><Button type="text" disabled={busy} aria-label="写作工作区操作" icon={<MoreOutlined/>}/></Dropdown><Button type="primary" aria-label="新建文档" icon={<PlusOutlined/>} disabled={busy} onClick={()=>void createPage()}>新建文档</Button></Space>
  </header>
  <div className="writing-layout">
   {directoryOpen&&<aside className="document-directory">
    <Input aria-label="搜索文档" prefix={<SearchOutlined/>} placeholder="搜索文档" value={query} onChange={e=>setQuery(e.target.value)} allowClear/>
    <div className="directory-caption"><span>我的文档</span><Tooltip title="新建图书"><Button type="text" size="small" aria-label="新建图书" icon={<PlusOutlined/>} onClick={createBook}/></Tooltip></div>
    <div className="document-tree"><Tree key={term?`search:${term}`:'normal'} showIcon blockNode selectedKeys={active?[`page:${active.id}`]:selectedBook?[`book:${selectedBook}`]:['root']} defaultExpandedKeys={term?['root',...books.map(b=>`book:${b.id}`)]:['root']} treeData={nodes} onSelect={keys=>{const key=String(keys[0]||'');if(key.startsWith('page:')){if(!canLeave())return;const p=pages.find(x=>x.id===key.slice(5));if(p){setActive({...p});setSelectedBook(p.book_id==='__personal_writing__'?null:p.book_id);setDirty(false)}}else if(key.startsWith('book:'))setSelectedBook(key.slice(5));else if(key==='root')setSelectedBook(null)}}/></div>
    <div className="directory-footer"><FileTextOutlined/> Markdown 文档</div>
   </aside>}
   <section className="document-workspace">
    {(error||draftError)&&<Alert className="writing-notice" type="error" showIcon closable message={error||draftError} onClose={()=>setError('')}/>}
    {notice&&<Alert className="writing-notice" type="success" closable message={notice} onClose={()=>setNotice('')}/>}
    {active?<>
     <div className="document-actions"><span className={`save-state ${dirty?'pending':''}`}>{busy?'正在保存…':dirty?'等待保存':<><CheckOutlined/> 已保存</>}</span><Space><WritingZoom/><Button type="text" disabled={!dirty||busy} onClick={()=>void save()}>保存</Button><Dropdown trigger={['click']} menu={{items:[{key:'export',label:'导出 Markdown'},{key:'copy',label:'另存副本'},{key:'draft',label:'保存草稿',disabled:!dirty},{key:'publish',label:'发布到博客'},{key:'publish-mongo',label:'发布到文档库',disabled:dirty},{key:'move',label:'移动文档'},{key:'tags',label:'管理标签'},{type:'divider'},{key:'delete',label:'删除文档',danger:true}],onClick:({key})=>{if(key==='export')void exportPage();else if(key==='copy')void saveCopy();else if(key==='draft')void draft();else if(key==='publish')void publishBlog();else if(key==='publish-mongo')void publishMongo();else if(key==='move')chooseMove('page',active.id,active.title);else if(key==='tags')setManageTags(true);else setRemoving({id:active.id,page:true})}}}><Button type="text" disabled={busy} aria-label="文档操作" icon={<MoreOutlined/>}/></Dropdown></Space></div>
     <div className="document-scroll"><article className="document-paper">
      <Input className="document-title" variant="borderless" aria-label="文档标题" disabled={busy} value={active.title} placeholder="未命名文档" onChange={e=>{setActive({...active,title:e.target.value});setDirty(true)}}/>
      <div className="document-metadata"><span>标签</span><Select variant="borderless" disabled={busy} aria-label="文档标签" mode="multiple" placeholder="添加标签" value={active.tags} options={tags.map(t=>({value:t,label:t}))} onChange={selected=>{setActive({...active,tags:selected});setDirty(true)}}/></div>
      <MarkdownEditor key={active.id} documentId={active.id} label="文档正文" disabled={busy} rows={22} value={active.body} onChange={body=>{setActive({...active,body});setDirty(true)}}/>
     </article></div>
     <footer className="document-status"><span>{Array.from(active.body).length} 字</span><span>自动保存 · Ctrl+S 保存</span><span>Ctrl+V 粘贴图片</span></footer>
    </>:<div className="writing-empty"><span className="writing-empty-icon"><FileTextOutlined/></span><h2>从一页空白开始</h2><p>记录想法、整理资料，让文字有自己的位置。</p><Button type="primary" icon={<PlusOutlined/>} onClick={()=>void createPage()}>写一篇文档</Button><span>也可以从左侧打开已有文档</span></div>}
   </section>
  </div>
  <Modal title={naming?.book?'重命名图书':'新建图书'} open={!!naming} onCancel={()=>setNaming(null)} onOk={()=>void saveBookName()} okText="保存" cancelText="取消" confirmLoading={busy}><Input aria-label="图书名称" placeholder="为图书命名" value={name} onChange={e=>setName(e.target.value)} onPressEnter={()=>void saveBookName()}/></Modal>
  <Modal title={removing?.page?'删除此文档？':'删除此图书及其子图书和页面？'} open={!!removing} onCancel={()=>setRemoving(null)} onOk={()=>{if(removing)void remove(removing.id,removing.page)}} okText="删除" cancelText="取消" okButtonProps={{danger:true}}><p>请确认要删除选中的内容。</p></Modal>
  <Modal title="恢复未完成草稿？" open={recoverOpen} onCancel={()=>setRecoverOpen(false)} onOk={()=>{setRecoverOpen(false);void recover()}} okText="恢复" cancelText="取消"><p>恢复内容将保存为独立文档。</p></Modal>
  <Modal title="管理文档标签" open={manageTags} onCancel={()=>setManageTags(false)} footer={<Button onClick={()=>setManageTags(false)}>完成</Button>}>{error&&<Alert type="error" message={error}/>}<Space.Compact style={{width:'100%'}}><Input aria-label="标签名称" value={newTag} onChange={e=>setNewTag(e.target.value)}/><Button onClick={()=>{if(newTag.trim())void changeTag('writing_tag_create',{name:newTag}).then(ok=>{if(ok)setNewTag('')})}}>添加</Button></Space.Compact>{tags.map(tag=><div className="tag-row" key={tag}><span>{tag}</span><Space><Button onClick={()=>{const name=window.prompt('修改标签名称',tag);if(name&&name!==tag)void changeTag('writing_tag_rename',{original:tag,name})}}>重命名</Button><Button danger onClick={()=>{if(window.confirm(`删除标签“${tag}”？文档不会删除。`))void changeTag('writing_tag_delete',{name:tag})}}>删除</Button></Space></div>)}</Modal><Modal title={`移动${moving?.kind==='book'?'图书':'文档'}：${moving?.title||''}`} open={!!moving} onCancel={()=>setMoving(null)} onOk={()=>void move()} confirmLoading={busy} okText="移动" cancelText="取消">{error&&<Alert type="error" message={error}/>}<Select aria-label="目标目录" value={destination} onChange={setDestination} style={{width:'100%',marginTop:12}} options={[...([{value:'',label:'文档根目录'}]),...books.filter(b=>b.id!==moving?.id).map(b=>({value:b.id,label:b.title}))]}/></Modal></div>
}
