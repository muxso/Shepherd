#!/usr/bin/env node
// Pivot the per-chunk translation outputs in .i18n-work/out.*.json
// (keyed by dot.key -> { lang: text }) into one file per language at
// web/src/locales/<lang>.json (keyed by dot.key -> text).
//
// Reports coverage against the DICT keys in .i18n-work/base.json so gaps are
// visible instead of silently shipping English.
//
// Usage: node scripts/i18n_pivot.mjs
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const WORK = path.join(ROOT, '.i18n-work')
const OUT = path.join(ROOT, 'web/src/locales')

const LANGS = ['ja', 'ko', 'fr', 'de', 'es', 'pt', 'ru', 'it', 'tr', 'vi', 'th', 'id', 'nl', 'pl', 'ar']

const base = JSON.parse(fs.readFileSync(path.join(WORK, 'base.json'), 'utf8'))
const allKeys = Object.keys(base)

const tables = Object.fromEntries(LANGS.map((l) => [l, {}]))
const seen = new Set()
const problems = []

// out.enonly.json is a flat key->en map consumed by i18n_merge_en.mjs, not a locale chunk
const files = fs.readdirSync(WORK).filter((f) => /^out\..*\.json$/.test(f) && f !== 'out.enonly.json').sort()
for (const f of files) {
  let data
  try {
    data = JSON.parse(fs.readFileSync(path.join(WORK, f), 'utf8'))
  } catch (e) {
    problems.push(`${f}: unparseable (${e.message})`)
    continue
  }
  for (const [key, byLang] of Object.entries(data)) {
    if (!(key in base)) {
      problems.push(`${f}: unknown key ${key}`)
      continue
    }
    seen.add(key)
    for (const l of LANGS) {
      const v = byLang?.[l]
      if (typeof v !== 'string' || !v.trim()) {
        problems.push(`${f}: ${key} missing ${l}`)
        continue
      }
      const trimmed = v.trim()
      // A translation that still equals the Chinese source means the model
      // echoed the input instead of translating it.
      if (l !== 'ja' && l !== 'ko' && trimmed === base[key].zh) {
        problems.push(`${f}: ${key} [${l}] untranslated (equals zh)`)
      }
      tables[l][key] = trimmed
    }
  }
}

fs.mkdirSync(OUT, { recursive: true })
for (const l of LANGS) {
  // Sort keys so regenerating one language produces a stable diff.
  const sorted = Object.fromEntries(Object.keys(tables[l]).sort().map((k) => [k, tables[l][k]]))
  fs.writeFileSync(path.join(OUT, `${l}.json`), JSON.stringify(sorted, null, 2) + '\n')
}

const missing = allKeys.filter((k) => !seen.has(k))
console.log(`chunk files : ${files.length}`)
console.log(`dict keys   : ${allKeys.length}`)
console.log(`translated  : ${seen.size}`)
console.log(`missing     : ${missing.length}`)
for (const l of LANGS) console.log(`  ${l}: ${Object.keys(tables[l]).length}`)
if (missing.length) fs.writeFileSync(path.join(WORK, 'untranslated.json'), JSON.stringify(missing, null, 2))
if (problems.length) {
  fs.writeFileSync(path.join(WORK, 'problems.txt'), problems.join('\n') + '\n')
  console.log(`problems    : ${problems.length} (see .i18n-work/problems.txt)`)
}
