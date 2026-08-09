import { useCallback, useEffect, useMemo, useState } from 'react'

// Conversation history for the knowledge-base chat, faithfully mirroring feishu-knowledge-base's
// useChatHistory: localStorage-backed, instant load, auto-title from the first user message, capped
// at 50 conversations × 100 messages. (feishu also dual-writes to a server; our RAG sessions are
// stateless, so this is the localStorage-only path — the same graceful mode feishu falls back to.)
// Scoped per project, since our chat is about a specific project's knowledge base.

export interface StoredMsg {
  role: 'user' | 'assistant'
  content: string
  citations?: unknown[]
}
export interface ChatConversation {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  messages: StoredMsg[]
}

const MAX_CONVERSATIONS = 50
const MAX_MESSAGES_PER_CONV = 100
const keyFor = (projectId: string) => `kb_chat_history_v1:${projectId || 'none'}`

function load(projectId: string): ChatConversation[] {
  try {
    const raw = localStorage.getItem(keyFor(projectId))
    const list = raw ? JSON.parse(raw) : []
    return Array.isArray(list) ? list : []
  } catch {
    return []
  }
}
function save(projectId: string, list: ChatConversation[]) {
  try {
    localStorage.setItem(keyFor(projectId), JSON.stringify(list.slice(0, MAX_CONVERSATIONS)))
  } catch {
    /* quota — ignore */
  }
}

export function newConversationId(): string {
  return (crypto?.randomUUID?.() ?? `c-${Date.now()}-${Math.round(Math.random() * 1e9)}`)
}

export function useChatHistory(projectId: string) {
  const [list, setList] = useState<ChatConversation[]>(() => load(projectId))
  useEffect(() => { setList(load(projectId)) }, [projectId])

  const sorted = useMemo(() => [...list].sort((a, b) => b.updatedAt - a.updatedAt), [list])

  const upsert = useCallback((id: string, messages: StoredMsg[]) => {
    setList((cur) => {
      if (!messages.length) return cur
      const now = Date.now()
      const title = messages.find((m) => m.role === 'user')?.content?.slice(0, 40) || '(新对话)'
      const trimmed = messages.slice(-MAX_MESSAGES_PER_CONV)
      const idx = cur.findIndex((c) => c.id === id)
      const conv: ChatConversation = {
        id, title, updatedAt: now,
        createdAt: idx >= 0 ? cur[idx].createdAt : now,
        messages: trimmed,
      }
      const next = idx >= 0
        ? cur.map((c, i) => (i === idx ? conv : c))
        : [conv, ...cur].slice(0, MAX_CONVERSATIONS)
      save(projectId, next)
      return next
    })
  }, [projectId])

  const remove = useCallback((id: string) => {
    setList((cur) => { const next = cur.filter((c) => c.id !== id); save(projectId, next); return next })
  }, [projectId])

  const clear = useCallback(() => { setList([]); save(projectId, []) }, [projectId])

  const get = useCallback((id: string) => list.find((c) => c.id === id) || null, [list])

  const recentQuestions = useCallback((limit = 3): string[] => {
    const seen = new Set<string>()
    const out: string[] = []
    for (const conv of sorted) {
      for (const m of conv.messages) {
        if (m.role === 'user') {
          const q = m.content.trim()
          if (q && !seen.has(q)) { seen.add(q); out.push(q); if (out.length >= limit) return out }
        }
      }
    }
    return out
  }, [sorted])

  return { list: sorted, upsert, remove, clear, get, recentQuestions }
}
