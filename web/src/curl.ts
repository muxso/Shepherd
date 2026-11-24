import type { ApiSpecKV } from './api'

export type CurlRequest = {
  method: string
  url: string
  headers: ApiSpecKV[]
  body: string | null
}

type Quote = 'single' | 'double' | 'ansi'
const ANSI_ESCAPES: Record<string, string> = {
  a: '\x07', b: '\b', e: '\x1b', E: '\x1b', f: '\f', n: '\n', r: '\r', t: '\t', v: '\v',
  '\\': '\\', "'": "'", '"': '"', '?': '?',
}

function shellWords(source: string): string[] | null {
  const words: string[] = []
  let token = ''
  let started = false
  let quote: Quote | undefined
  for (let i = 0; i < source.length; i++) {
    const c = source[i]
    if (quote === 'single') {
      if (c === "'") quote = undefined
      else token += c
      continue
    }
    if (c === '\\') {
      const next = source[i + 1]
      if (quote === 'ansi') {
        if (next === undefined) return null
        const encoded = /^(?:x([\da-fA-F]{1,2})|u([\da-fA-F]{1,4})|U([\da-fA-F]{1,8})|([0-7]{1,3}))/.exec(source.slice(i + 1))
        if (encoded) {
          const codepoint = Number.parseInt(encoded[1] || encoded[2] || encoded[3] || encoded[4], encoded[4] ? 8 : 16)
          if (codepoint > 0x10ffff) return null
          token += String.fromCodePoint(codepoint)
          i += encoded[0].length
        } else {
          token += ANSI_ESCAPES[next] ?? `\\${next}`
          i++
        }
        continue
      }
      if (next === '\n' || (next === '\r' && source[i + 2] === '\n')) {
        i += next === '\r' ? 2 : 1
        continue
      }
      // Pasted commands sometimes replace a line break after a standalone backslash with spaces.
      if (!quote && !started && /^\\[ \t]+(?:-|https?:\/\/)/.test(source.slice(i))) continue
      if (next === undefined) return null
      if (!quote || ['"', '\\', '$', '`'].includes(next)) {
        token += next
        started = true
        i++
      } else {
        token += c
      }
      continue
    }
    if (quote) {
      if ((quote === 'double' && c === '"') || (quote === 'ansi' && c === "'")) quote = undefined
      else token += c
    } else if (c === "'" || c === '"') {
      quote = c === "'" ? 'single' : 'double'
      started = true
    } else if (c === '$' && source[i + 1] === "'") {
      quote = 'ansi'
      started = true
      i++
    } else if (/\s/.test(c)) {
      if (started) words.push(token)
      token = ''
      started = false
    } else {
      if ('|;<>'.includes(c)) return null
      token += c
      started = true
    }
  }
  if (quote) return null
  if (started) words.push(token)
  return words
}

const VALUE_OPTIONS: Record<string, string> = {
  '-X': '--request', '-H': '--header', '-d': '--data', '-u': '--user', '-b': '--cookie',
  '-A': '--user-agent', '-e': '--referer',
}
const DATA_OPTIONS = new Set(['--data', '--data-ascii', '--data-raw', '--data-binary', '--data-urlencode', '--json'])
const VALUE_FLAGS = new Set([
  '--request', '--header', '--user', '--cookie', '--user-agent', '--referer', '--url', '--oauth2-bearer', ...DATA_OPTIONS,
])
const SAFE_FLAGS = new Set(['--compressed', '--globoff', '--insecure', '--location', '--location-trusted', '--silent', '--show-error', '--verbose', '--fail', '--fail-with-body', '--http1.1', '--http2', '--http2-prior-knowledge'])

/** Read an inline HTTP request without executing the command or reading referenced local files. */
export function parseCurl(text: string): CurlRequest | null {
  const tokens = shellWords(text.trim())
  if (!tokens || !/^(?:curl|curl\.exe)$/i.test(tokens[0] ?? '')) return null

  let method = ''
  let url = ''
  let queryData = false
  let json = false
  const headers: ApiSpecKV[] = []
  const data: string[] = []
  const header = (name: string, value: string) => headers.push({ name, value, desc: '' })
  for (let i = 1; i < tokens.length; i++) {
    const token = tokens[i]
    if (/^https?:\/\//i.test(token)) {
      if (url) return null
      url = token
      continue
    }
    if (token === '-G' || token === '--get') {
      queryData = true
      continue
    }
    if (token === '-I' || token === '--head') {
      method ||= 'HEAD'
      continue
    }
    if (SAFE_FLAGS.has(token) || /^-[ksSLvgf]+$/.test(token)) continue

    const longOption = /^(--[^=]+)(?:=([\s\S]*))?$/.exec(token)
    const short = token.slice(0, 2)
    const option = longOption?.[1] ?? VALUE_OPTIONS[short]
    if (!VALUE_FLAGS.has(option)) return null
    const inline = longOption ? longOption[2] : token.slice(2) || undefined
    const value = inline ?? tokens[++i]
    if (value === undefined) return null

    if (DATA_OPTIONS.has(option)) {
      // In file-reading data modes, @filename is an input file, not request text.
      if (option !== '--data-raw' && value.startsWith('@')) return null
      if (option === '--data-urlencode') {
        const equal = value.indexOf('=')
        if (equal < 0 && value.includes('@')) return null
        data.push(equal < 0 ? encodeURIComponent(value) : `${value.slice(0, equal + 1)}${encodeURIComponent(value.slice(equal + 1))}`)
      } else {
        data.push(value)
      }
      json ||= option === '--json'
      continue
    }
    switch (option) {
      case '--url':
        if (url || !/^https?:\/\//i.test(value)) return null
        url = value
        break
      case '--request':
        method = value.toUpperCase()
        break
      case '--header': {
        const colon = value.indexOf(':')
        if (colon <= 0) return null
        header(value.slice(0, colon).trim(), value.slice(colon + 1).trim())
        break
      }
      case '--user': {
        if (!value.includes(':')) return null
        const bytes = new TextEncoder().encode(value)
        header('Authorization', `Basic ${btoa(Array.from(bytes, (byte) => String.fromCharCode(byte)).join(''))}`)
        break
      }
      case '--cookie':
        if (!value.includes('=')) return null
        header('Cookie', value)
        break
      case '--user-agent':
        header('User-Agent', value)
        break
      case '--referer':
        header('Referer', value)
        break
      case '--oauth2-bearer':
        header('Authorization', `Bearer ${value}`)
        break
    }
  }

  try {
    if (!/^https?:\/\//i.test(url)) return null
    new URL(url)
  } catch {
    return null
  }
  const payload = data.length ? data.join(json ? '' : '&') : null
  if (queryData && payload) {
    const hash = url.indexOf('#')
    const base = hash < 0 ? url : url.slice(0, hash)
    url = `${base}${base.endsWith('?') || base.endsWith('&') ? '' : base.includes('?') ? '&' : '?'}${payload}${hash < 0 ? '' : url.slice(hash)}`
  }
  if (payload !== null && !queryData && !headers.some(({ name }) => name.toLowerCase() === 'content-type')) {
    header('Content-Type', json ? 'application/json' : 'application/x-www-form-urlencoded')
  }
  if (json && !headers.some(({ name }) => name.toLowerCase() === 'accept')) header('Accept', 'application/json')
  return { method: method || (payload !== null && !queryData ? 'POST' : 'GET'), url, headers, body: queryData ? null : payload }
}
