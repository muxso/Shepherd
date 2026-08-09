#!/usr/bin/env node
// Reconcile web/src/i18n.tsx's DICT with the `t('key', 'fallback')` call sites
// in web/src:
//
//   1. Entries whose `zh` is empty get it filled from the call-site fallback.
//   2. Keys used at a call site but absent from DICT are appended.
//   3. Any non-zh/en language fields still inlined in DICT are lifted out into
//      .i18n-work/out.existing.json so the pivot step can fold them into
//      web/src/locales/ instead of leaving them stranded in the .tsx.
//
// zh/en stay inline in DICT — they are the source of truth. Everything else
// belongs in the locale files.
//
// Usage: node scripts/i18n_repair.mjs [--dry]
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { execFileSync } from 'node:child_process'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const SRC = path.join(ROOT, 'web/src/i18n.tsx')
const WORK = path.join(ROOT, '.i18n-work')
const DRY = process.argv.includes('--dry')

const SOURCE_LANGS = new Set(['zh', 'en'])

// --- call-site fallbacks -----------------------------------------------------
const fallbacks = JSON.parse(
  execFileSync('node', [path.join(ROOT, 'scripts/i18n_fallbacks.mjs')], { encoding: 'utf8' }),
)

// --- rewrite DICT entry lines ------------------------------------------------
const text = fs.readFileSync(SRC, 'utf8')
const lines = text.split('\n')

// A couple of entries with long values are wrapped across several lines, so the
// per-line rewrite below cannot see them. Evaluate the whole object literal to
// get the authoritative key set — otherwise those keys look absent and get
// appended a second time.
const dictKeys = new Set(Object.keys(JSON.parse(fs.readFileSync(path.join(WORK, 'base.json'), 'utf8'))))

// `  'key': { zh: '...', en: '...' },`
const ENTRY = /^(\s*)((['"])(?:\\.|(?!\3).)*\3)\s*:\s*(\{.*\})\s*,?\s*$/

/** Serialise a string as a JS literal, preferring single quotes. */
function q(s) {
  const esc = s.replace(/\\/g, '\\\\').replace(/\n/g, '\\n').replace(/\r/g, '\\r')
  if (!esc.includes("'")) return `'${esc}'`
  if (!esc.includes('"')) return `"${esc}"`
  return `'${esc.replace(/'/g, "\\'")}'`
}

const lifted = {} // key -> { lang: text } for languages other than zh/en
const seen = new Set()
const filledZh = []
const stillMissing = []
let liftedCount = 0

for (let i = 0; i < lines.length; i++) {
  const m = ENTRY.exec(lines[i])
  if (!m) continue
  const [, indent, rawKey, , objSrc] = m
  let key
  let obj
  try {
    key = new Function('return ' + rawKey)()
    obj = new Function('return ' + objSrc)()
  } catch {
    continue
  }
  if (typeof obj !== 'object' || obj === null || Array.isArray(obj)) continue
  seen.add(key)

  // Lift out non-source languages.
  const extra = {}
  for (const [l, v] of Object.entries(obj)) {
    if (SOURCE_LANGS.has(l)) continue
    if (typeof v === 'string' && v.trim()) extra[l] = v.trim()
  }
  if (Object.keys(extra).length) {
    lifted[key] = extra
    liftedCount++
  }

  let zh = typeof obj.zh === 'string' ? obj.zh : ''
  const en = typeof obj.en === 'string' ? obj.en : ''
  if (!zh && fallbacks[key]) {
    zh = fallbacks[key]
    filledZh.push(key)
  }
  if (!zh || !en) stillMissing.push(key)

  // Leave already-correct lines byte-identical so the diff only shows real
  // changes rather than a wholesale requoting of the file.
  const untouched =
    !Object.keys(extra).length && zh === obj.zh && Object.keys(obj).length === 2 && 'zh' in obj && 'en' in obj
  if (untouched) continue

  lines[i] = `${indent}${rawKey}: { zh: ${q(zh)}, en: ${q(en)} },`
}

// --- append keys that exist only at call sites -------------------------------
const added = Object.keys(fallbacks).filter((k) => !seen.has(k) && !dictKeys.has(k)).sort()
if (added.length) {
  // Find the depth-0 closing brace of the DICT object literal.
  let close = -1
  let depth = 0
  let started = false
  for (let i = 0; i < lines.length; i++) {
    if (!started && !/^const DICT/.test(lines[i])) continue
    started = true
    for (const c of lines[i]) {
      if (c === '{') depth++
      else if (c === '}') {
        depth--
        if (depth === 0) {
          close = i
          break
        }
      }
    }
    if (close >= 0) break
  }
  if (close < 0) throw new Error('could not locate end of DICT')
  const block = [
    '  // Keys recovered from t() call sites that had no dictionary entry.',
    ...added.map((k) => `  ${q(k)}: { zh: ${q(fallbacks[k])}, en: '' },`),
  ]
  lines.splice(close, 0, ...block)
  stillMissing.push(...added)
}

if (!DRY) {
  fs.writeFileSync(SRC, lines.join('\n'))
  fs.mkdirSync(WORK, { recursive: true })
  fs.writeFileSync(path.join(WORK, 'out.existing.json'), JSON.stringify(lifted, null, 2))
  fs.writeFileSync(path.join(WORK, 'needs-source.json'), JSON.stringify([...new Set(stillMissing)].sort(), null, 2))
}

console.log(`call sites      : ${Object.keys(fallbacks).length}`)
console.log(`dict entries    : ${seen.size}`)
console.log(`zh filled in    : ${filledZh.length}`)
console.log(`keys appended   : ${added.length}`)
console.log(`langs lifted out: ${liftedCount} entries -> .i18n-work/out.existing.json`)
console.log(`still incomplete: ${new Set(stillMissing).size} -> .i18n-work/needs-source.json`)
if (added.length) console.log(`  appended: ${added.slice(0, 12).join(', ')}${added.length > 12 ? ' …' : ''}`)
