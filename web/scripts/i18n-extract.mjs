// Extract zh fallback for every t('key', 'zh-fallback') usage, restrict to keys
// missing from DICT, and emit JSON [{key, zh}] sorted. Keys with no fallback in
// code are reported separately.
import { readFileSync, readdirSync, statSync, writeFileSync } from 'fs'
import { join } from 'path'

const SRC = new URL('../src', import.meta.url).pathname
const i18n = readFileSync(join(SRC, 'i18n.tsx'), 'utf8')
const dictKeys = new Set()
for (const m of i18n.matchAll(/^\s*['"]([a-zA-Z0-9_.]+)['"]\s*:\s*\{\s*zh:/gm)) dictKeys.add(m[1])

function walk(dir) {
  let out = []
  for (const f of readdirSync(dir)) {
    const p = join(dir, f)
    if (statSync(p).isDirectory()) out = out.concat(walk(p))
    else if (/\.tsx?$/.test(f)) out.push(p)
  }
  return out
}

// Match t('key', 'fallback' | "fallback" | `fallback`) — capture fallback (no nested same-quote).
const re = /\bt\(\s*['"]([a-zA-Z0-9_.]+)['"]\s*,\s*(['"`])((?:\\.|(?!\2).)*)\2/g
const zh = new Map()
const used = new Set()
const usedNoFallback = new Set()
for (const file of walk(SRC)) {
  if (file.endsWith('i18n.tsx')) continue
  const txt = readFileSync(file, 'utf8')
  for (const m of txt.matchAll(/\bt\(\s*['"]([a-zA-Z0-9_.]+)['"]/g)) used.add(m[1])
  for (const m of txt.matchAll(re)) {
    const [, key, , val] = m
    if (!zh.has(key)) zh.set(key, val)
  }
}

const missing = [...used].filter((k) => !dictKeys.has(k)).sort()
const withZh = []
const noZh = []
for (const k of missing) {
  if (zh.has(k)) withZh.push({ key: k, zh: zh.get(k) })
  else noZh.push(k)
}
writeFileSync(join(SRC, '..', 'scripts', 'missing-with-zh.json'), JSON.stringify(withZh, null, 0))
console.log('missing total:', missing.length, '| with zh fallback:', withZh.length, '| NO fallback:', noZh.length)
console.log('NO-FALLBACK KEYS:', noZh.join(', '))
