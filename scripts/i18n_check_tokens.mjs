#!/usr/bin/env node
// Verify that every locale translation preserves the placeholder tokens of its source.
// Tokens: ${...} template expressions and {name} interpolation slots.
// Usage: node scripts/i18n_check_tokens.mjs [--json]
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const base = JSON.parse(fs.readFileSync(path.join(ROOT, '.i18n-work/base.json'), 'utf8'))
const LOC = path.join(ROOT, 'web/src/locales')
const LANGS = ['ja', 'ko', 'fr', 'de', 'es', 'pt', 'ru', 'it', 'tr', 'vi', 'th', 'id', 'nl', 'pl', 'ar']

const TOKEN = /\$\{[^}]*\}|\{\w+\}/g
const tokens = (s) => (String(s || '').match(TOKEN) || []).slice().sort()
const eq = (a, b) => a.length === b.length && a.every((x, i) => x === b[i])

const locales = Object.fromEntries(LANGS.map((l) => [l, JSON.parse(fs.readFileSync(path.join(LOC, `${l}.json`), 'utf8'))]))

const bad = []
for (const [key, v] of Object.entries(base)) {
  const want = tokens(v.en && v.en.trim() ? v.en : v.zh)
  if (!want.length) continue
  for (const l of LANGS) {
    const got = tokens(locales[l][key])
    if (!eq(want, got)) bad.push({ key, lang: l, want, got, text: locales[l][key] })
  }
}

const byKey = new Map()
for (const b of bad) {
  if (!byKey.has(b.key)) byKey.set(b.key, [])
  byKey.get(b.key).push(b)
}

if (process.argv.includes('--json')) {
  console.log(JSON.stringify([...byKey.keys()], null, 2))
} else {
  console.log('keys with token mismatch:', byKey.size, '/ occurrences:', bad.length)
  for (const [key, list] of byKey) {
    const src = base[key].en || base[key].zh
    console.log(`\n${key}  src=${JSON.stringify(src)} want=${JSON.stringify(list[0].want)}`)
    console.log('  langs:', list.map((x) => x.lang).join(' '))
    console.log('  e.g.', list[0].lang, '=', JSON.stringify(list[0].text))
  }
}
process.exit(byKey.size ? 1 : 0)
