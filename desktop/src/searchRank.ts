export type SearchField = { value: string | null | undefined; weight?: number }

function normalize(value: string) {
  return value.normalize('NFKC').toLocaleLowerCase()
}

function isWordStart(text: string, index: number) {
  if (index === 0) return true
  return /[\s\-_/\\.,;:!?()[\]{}<>"'`~@#$%^&*+=|，。；：！？、（）【】《》“”‘’·]/.test(text[index - 1])
}

type NormalizedField = { text: string; weight: number }

function normalizeFields(fields: SearchField[]): NormalizedField[] {
  return fields.flatMap(({ value, weight = 1 }) => value ? [{ text: normalize(value), weight }] : [])
}

function scoreTerm(term: string, fields: NormalizedField[]) {
  let best = 0
  for (const { text, weight } of fields) {
    if (best >= 1000 * weight) continue
    const index = text.indexOf(term)
    if (index === -1) continue
    const base = text === term ? 1000 : index === 0 ? 700 : isWordStart(text, index) ? 550 : 300
    best = Math.max(best, base * weight)
  }
  return best
}

/**
 * 相关度评分：标题精确匹配 > 标题前缀 > 词首 > 任意位置，再乘以字段权重。
 * 多个关键词需要全部命中，分数累加。
 */
export function searchScore(query: string, fields: SearchField[]) {
  return scoreTerms(searchTerms(query), normalizeFields(fields))
}

function searchTerms(query: string) {
  return normalize(query).split(/\s+/).filter(Boolean)
}

function scoreTerms(terms: string[], fields: NormalizedField[]) {
  if (!terms.length) return 0
  let total = 0
  for (const term of terms) {
    const score = scoreTerm(term, fields)
    if (!score) return 0
    total += score
  }
  return total
}

/**
 * 过滤并排序搜索结果。同分时先按 tieBreak，再保持原有顺序。
 */
export function rankBySearch<T>(items: T[], query: string, fields: (item: T) => SearchField[], tieBreak?: (a: T, b: T) => number): T[] {
  return scoredBySearch(items, query, fields, tieBreak).map(entry => entry.item)
}

export function scoredBySearch<T>(items: T[], query: string, fields: (item: T) => SearchField[], tieBreak?: (a: T, b: T) => number) {
  const terms = searchTerms(query)
  const ranked: { item: T; score: number }[] = []
  for (const item of items) {
    const score = terms.length ? scoreTerms(terms, normalizeFields(fields(item))) : 0
    if (!terms.length || score > 0) ranked.push({ item, score })
  }
  return ranked.sort((a, b) => b.score - a.score || (tieBreak?.(a.item, b.item) ?? 0))
}
