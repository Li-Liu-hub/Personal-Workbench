import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

type RecordValue = string | number | string[] | null
type Item = Record<string, RecordValue> & { id: string }
type Definition = { title: string; prefix: string; fields: { key: string; label: string; number?: boolean }[]; defaults: Omit<Item, 'id'> }
const definitions: Record<string, Definition> = {
  contacts: { title:'联系人', prefix:'contacts', fields:[{key:'name',label:'姓名'},{key:'school',label:'学校'},{key:'major',label:'专业'},{key:'work',label:'工作'},{key:'phone',label:'电话'},{key:'national_id',label:'证件号码'},{key:'address',label:'地址'},{key:'gender',label:'性别'},{key:'birthday',label:'生日'},{key:'hobbies',label:'爱好'}], defaults:{name:'',school:'',major:'',work:'',phone:'',national_id:'',address:'',gender:'',birthday:'',hobbies:'',chat_notes:'',chats:[]} },
  inventory: { title: '物品清单', prefix: 'inventory', fields: [
    {key:'name',label:'名称'}, {key:'category',label:'分类'}, {key:'quantity',label:'数量',number:true}, {key:'location',label:'位置'}, {key:'brand',label:'品牌'}, {key:'model',label:'型号'}, {key:'purchase_date',label:'购买日期'}, {key:'disposal_date',label:'处置日期'}, {key:'purchase_price_cents',label:'购买价格（分）',number:true}, {key:'notes',label:'备注'}
  ], defaults: {name:'',category:'',quantity:1,location:'',brand:'',model:'',purchase_date:'',disposal_date:'',purchase_price_cents:null,notes:'',tags:[]} },
  websites: { title:'网站收藏', prefix:'websites', fields:[{key:'title',label:'名称'},{key:'url',label:'网址'},{key:'category',label:'分类'},{key:'notes',label:'备注'}], defaults:{title:'',url:'',category:'',notes:''} },
  tools: { title:'小工具', prefix:'tools', fields:[{key:'name',label:'名称'},{key:'path',label:'程序路径'},{key:'notes',label:'备注'}], defaults:{name:'',path:'',notes:''} }
}
export function Collections({ section }: { section: string }) {
  const def = definitions[section]; const [items,setItems] = useState<Item[]>([]); const [draft,setDraft] = useState<Item|null>(null); const [error,setError] = useState('')
  async function refresh() { try { setItems(await invoke<Item[]>(`${def.prefix}_list`)); setError('') } catch(e) {setError(String(e))} }
  useEffect(() => { setDraft(null); setItems([]); void refresh() }, [section])
  async function save() { try { await invoke(`${def.prefix}_save`,{entry:draft}); setDraft(null); await refresh() } catch(e) {setError(String(e))} }
  async function remove(item:Item) { if (!window.confirm('确定删除这条记录吗？')) return; try {await invoke(`${def.prefix}_delete`,{id:item.id}); await refresh()} catch(e) {setError(String(e))} }
  async function launch(item:Item) {try {await invoke('tools_launch',{path:item.path})} catch(e) {setError(String(e))}}
  return <><header><h1>{def.title}</h1><button onClick={() => setDraft({ ...def.defaults, id:crypto.randomUUID() })}>新增</button></header>{error && <p role="alert" className="error">{error}</p>}
    {draft && <section className="editor">{def.fields.map(field => <label key={field.key}>{field.label}<input type={field.number ? 'number':'text'} value={String(draft[field.key] ?? '')} onChange={e => setDraft({...draft,[field.key]:field.number ? (e.target.value === '' ? null : Number(e.target.value)) : e.target.value})}/></label>)}<button onClick={() => void save()}>保存</button><button onClick={() => setDraft(null)}>取消</button></section>}
    <section className="cards">{items.map(item => <article key={item.id}><div className="card-head"><h2>{String(item.name || item.title || '未命名')}</h2><div><button onClick={() => setDraft({...item})}>编辑</button><button onClick={() => void remove(item)}>删除</button></div></div>{def.fields.slice(1).map(field => <p key={field.key}>{field.label}：{String(item[field.key] ?? '')}</p>)}{section === 'tools' && <button onClick={() => void launch(item)}>启动</button>}</article>)}</section>
  </>
}
