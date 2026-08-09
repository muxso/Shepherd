#!/usr/bin/env node
// Fill empty `en: ''` values in web/src/i18n.tsx DICT from translation outputs.
// Sources: .i18n-work/out.enonly.json (flat key -> en)
//          .i18n-work/out.pend*.json  (key -> { en, ja, ... })
// Usage: node scripts/i18n_merge_en.mjs [--dry]
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const WORK = path.join(ROOT, '.i18n-work')
const FILE = path.join(ROOT, 'web/src/i18n.tsx')
const DRY = process.argv.includes('--dry')

const en = {}
const read = (f) => JSON.parse(fs.readFileSync(path.join(WORK, f), 'utf8'))

for (const f of fs.readdirSync(WORK)) {
  if (f === 'out.enonly.json') {
    for (const [k, v] of Object.entries(read(f))) if (typeof v === 'string' && v.trim()) en[k] = v
  } else if (/^out\.pend[A-Z]\.json$/.test(f)) {
    for (const [k, v] of Object.entries(read(f))) if (v && typeof v.en === 'string' && v.en.trim()) en[k] = v.en
  }
}
console.log('en translations available:', Object.keys(en).length)

const lines = fs.readFileSync(FILE, 'utf8').split('\n')
const ENTRY = /^(\s*)(['"])((?:\\.|(?!\2).)*)\2\s*:\s*\{(.*)\}(,?)\s*$/
const EMPTY_EN = /\ben\s*:\s*(['"])\1/

let filled = 0
const missed = []
for (let i = 0; i < lines.length; i++) {
  const m = ENTRY.exec(lines[i])
  if (!m) continue
  const key = m[3]
  if (!(key in en)) continue
  if (!EMPTY_EN.test(m[4])) continue
  const body = m[4].replace(EMPTY_EN, `en: ${JSON.stringify(en[key])}`)
  lines[i] = `${m[1]}${m[2]}${key}${m[2]}: {${body}}${m[5]}`
  filled++
}
for (const k of Object.keys(en)) {
  if (!lines.some((l) => l.includes(`${JSON.stringify(en[k])}`))) missed.push(k)
}

console.log('en filled in i18n.tsx:', filled)
if (missed.length) console.log('not applied (check manually):', missed.length, missed.slice(0, 10))
if (DRY) { console.log('(dry run, nothing written)'); process.exit(0) }
fs.writeFileSync(FILE, lines.join('\n'))
console.log('written', FILE)
