export type Section =
  | 'today'
  | 'writing'
  | 'essays'
  | 'contacts'
  | 'credentials'
  | 'inventory'
  | 'folders'
  | 'websites'
  | 'tools'
  | 'settings'

export type FocusTarget = {
  id?: string
  intent?: 'start-essay' | 'essay-history' | 'new-page'
}

export type FocusState = FocusTarget & {
  section: Section
  nonce: number
}

export type Navigate = (section: Section, target?: FocusTarget) => boolean | Promise<boolean>
