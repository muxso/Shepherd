#!/usr/bin/env node
// Extract the DICT object literal from web/src/i18n.tsx into JSON, and split
// it into fixed-size chunk files for parallel translation.
//
// Usage: node scripts/i18n_extract.mjs [chunkSize]
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const SRC = path.join(ROOT, 'web/src/i18n.tsx')
const WORK = path.join(ROOT, '.i18n-work')

const CHUNK = Number(process.argv[2] || 100)

const text = fs.readFileSync(SRC, 'utf8')

// Locate `const DICT ... = {` and its matching depth-0 closing brace.
const declIdx = text.indexOf('const DICT')
if (declIdx < 0) throw new Error('const DICT not found')
const eqIdx = text.indexOf('=', declIdx)
const openIdx = text.indexOf('{', eqIdx)
if (openIdx < 0) throw new Error('DICT opening brace not found')

// Brace scan that respects quotes, template literals and comments.
function findClose(s, start) {
  let depth = 0
  let i = start
  while (i < s.length) {
    const c = s[i]
    if (c === '/' && s[i + 1] === '/') {
      i = s.indexOf('\n', i)
      if (i < 0) break
      continue
    }
    if (c === '/' && s[i + 1] === '*') {
      i = s.indexOf('*/', i) + 2
      continue
    }
    if (c === "'" || c === '"' || c === '`') {
      const q = c
      i++
      while (i < s.length) {
        if (s[i] === '\\') { i += 2; continue }
        if (s[i] === q) break
        i++
      }
      i++
      continue
    }
    if (c === '{') depth++
    else if (c === '}') {
      depth--
      if (depth === 0) return i
    }
    i++
  }
  throw new Error('unbalanced braces')
}

const closeIdx = findClose(text, openIdx)
const body = text.slice(openIdx, closeIdx + 1)

// The body is a plain JS object literal (no TS annotations inside), so it can
// be evaluated directly. This handles escapes/quotes correctly.
const dict = new Function('return ' + body)()

const keys = Object.keys(dict)
fs.mkdirSync(WORK, { recursive: true })
fs.writeFileSync(path.join(WORK, 'base.json'), JSON.stringify(dict, null, 2))

// Only keys with both zh and en can be translated; the rest need their source
// text repaired first (see scripts/i18n_fallbacks.mjs).
const ready = keys.filter((k) => dict[k].zh && dict[k].en)
const pending = keys.filter((k) => !dict[k].zh || !dict[k].en)

// Chunk files contain only zh/en source text — the translation input.
let n = 0
for (let i = 0; i < ready.length; i += CHUNK) {
  const out = {}
  for (const k of ready.slice(i, i + CHUNK)) out[k] = { zh: dict[k].zh, en: dict[k].en }
  const id = String(n).padStart(2, '0')
  fs.writeFileSync(path.join(WORK, `in.${id}.json`), JSON.stringify(out, null, 2))
  n++
}

console.log(`keys=${keys.length} ready=${ready.length} pending=${pending.length}`)
console.log(`chunks=${n} chunkSize=${CHUNK}`)
console.log(`dictRange=[${openIdx},${closeIdx}]`)
if (pending.length) fs.writeFileSync(path.join(WORK, 'pending.json'), JSON.stringify(pending, null, 2))
fs.writeFileSync(path.join(WORK, 'missing-zh.json'), JSON.stringify(keys.filter((k) => !dict[k].zh), null, 2))
