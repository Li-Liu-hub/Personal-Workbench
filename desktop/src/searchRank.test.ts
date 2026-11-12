import { describe, expect, it } from 'vitest'
import { rankBySearch, searchScore } from './searchRank'

const credentialFields = (item: { title: string; username?: string; website?: string; notes?: string; tags?: string[] }) => [
  { value: item.title, weight: 3 },
  { value: item.username, weight: 2 },
  { value: item.website, weight: 1.5 },
  { value: item.tags?.join(' '), weight: 1.2 },
  { value: item.notes, weight: 0.6 },
]

describe('searchScore', () => {
  it('标题前缀优先于标题中间命中', () => {
    expect(searchScore('a', [{ value: 'aTrust', weight: 3 }])).toBeGreaterThan(searchScore('a', [{ value: 'ChatGPT', weight: 3 }]))
  })

  it('标题命中优先于备注命中', () => {
    expect(searchScore('atrust', [{ value: 'aTrust', weight: 3 }])).toBeGreaterThan(searchScore('atrust', [{ value: '登录 aTrust 使用', weight: 0.6 }]))
  })

  it('多个关键词必须全部命中', () => {
    expect(searchScore('工作 邮箱', [{ value: '工作邮箱' }])).toBeGreaterThan(0)
    expect(searchScore('工作 微信', [{ value: '工作邮箱' }])).toBe(0)
  })
})

describe('rankBySearch', () => {
  it('搜索 a 时 aTrust 排在 ChatGPT 之前', () => {
    const items = [{ title: 'ChatGPT' }, { title: 'aTrust' }]
    expect(rankBySearch(items, 'a', credentialFields).map(item => item.title)).toEqual(['aTrust', 'ChatGPT'])
  })

  it('同分时置顶记录优先', () => {
    const items = [{ id: 'a', title: '邮箱' }, { id: 'b', title: '邮箱' }]
    expect(rankBySearch(items, '邮箱', item => [{ value: item.title }], (a, b) => Number(b.id === 'b') - Number(a.id === 'b')).map(item => item.id)).toEqual(['b', 'a'])
  })
})