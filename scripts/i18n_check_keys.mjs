#!/usr/bin/env node
// Find i18n keys referenced by the app but absent from the DICT in web/src/i18n.tsx.
// Such keys silently fall back to their hardcoded Chinese literal in every language,
// which is invisible in a build and only shows up when someone switches locale.
// Covers both `t('key', '中文')` calls and the `label: ['key', '中文']` tables used by
// AppShell's nav and similar module-scope metadata.
// Usage: node scripts/i18n_check_keys.mjs
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const SRC = path.join(ROOT, 'web/src')
const base = JSON.parse(fs.readFileSync(path.join(ROOT, '.i18n-work/base.json'), 'utf8'))

const files = []
;(function walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name)
    if (e.isDirectory()) { if (e.name !== 'locales') walk(p) } else if (/\.tsx?$/.test(e.name)) files.push(p)
  }
})(SRC)

const RE_T = /\bt\(\s*(['"])((?:\\.|(?!\1).)*)\1/g
const RE_LABEL = /label:\s*\[\s*(['"])((?:\\.|(?!\1).)*)\1/g

const used = new Map()
for (const f of files) {
  const s = fs.readFileSync(f, 'utf8')
  for (const re of [RE_T, RE_LABEL]) {
    re.lastIndex = 0
    let m
    while ((m = re.exec(s))) {
      const key = m[2]
      // Skip dynamically built keys and bare words that aren't dotted key names.
      if (key.includes('${') || !key.includes('.')) continue
      if (!used.has(key)) used.set(key, [])
      used.get(key).push(path.relative(ROOT, f))
    }
  }
}

const missing = [...used.keys()].filter((k) => !base[k]).sort()
console.log('keys referenced:', used.size, '| missing from DICT:', missing.length)
for (const k of missing) console.log(` ${k}  <- ${[...new Set(used.get(k))].join(', ')}`)
process.exit(missing.length ? 1 : 0)
