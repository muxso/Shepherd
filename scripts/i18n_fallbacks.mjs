#!/usr/bin/env node
// Scan web/src for `t('key', 'fallback')` call sites and report the fallback
// literal for each key. Used to recover the zh source text for DICT entries
// whose zh field is empty.
//
// Usage: node scripts/i18n_fallbacks.mjs [keysJson] > out.json
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const SRC = path.join(ROOT, 'web/src')

function walk(dir, acc = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name)
    if (e.isDirectory()) walk(p, acc)
    else if (/\.(tsx?|jsx?)$/.test(e.name)) acc.push(p)
  }
  return acc
}

// t('key', 'fallback')  — the optional third `vars` argument means the call may
// continue with a comma instead of closing here.
const RE = /\bt\(\s*(['"])((?:\\.|(?!\1).)*)\1\s*,\s*(['"`])((?:\\.|(?!\3)[\s\S])*)\3\s*[,)]/g

// Module-scope tables that cannot call the hook pair the key with its Chinese
// source instead, e.g. `{ labelKey: 'rag.stepRerank', labelZh: '重排' }`.
const RE_PAIR = /\bl(?:abel)?Key\s*:\s*(['"])((?:\\.|(?!\1).)*)\1\s*,\s*l(?:abel)?Zh\s*:\s*(['"])((?:\\.|(?!\3).)*)\3/g

const found = {}
const conflicts = {}
for (const file of walk(SRC)) {
  const text = fs.readFileSync(file, 'utf8')
  for (const re of [RE, RE_PAIR]) {
    re.lastIndex = 0
    let m
    while ((m = re.exec(text))) {
      const key = m[2]
      const raw = m[4]
      // Unescape the JS string literal.
      let val
      try {
        val = JSON.parse('"' + raw.replace(/"/g, '\\"').replace(/\\'/g, "'") + '"')
      } catch {
        val = raw
      }
      if (found[key] && found[key] !== val) {
        ;(conflicts[key] ||= new Set()).add(found[key]).add(val)
      }
      found[key] ??= val
    }
  }
}

const wanted = process.argv[2] ? JSON.parse(fs.readFileSync(process.argv[2], 'utf8')) : null
const out = {}
for (const [k, v] of Object.entries(found)) {
  if (wanted && !wanted.includes(k)) continue
  out[k] = v
}

process.stdout.write(JSON.stringify(out, null, 2) + '\n')
const conflictKeys = Object.keys(conflicts).filter((k) => !wanted || wanted.includes(k))
if (conflictKeys.length) {
  console.error(`conflicting fallbacks for ${conflictKeys.length} keys:`)
  for (const k of conflictKeys) console.error(`  ${k}: ${[...conflicts[k]].join(' | ')}`)
}
console.error(`resolved ${Object.keys(out).length} keys`)
if (wanted) {
  const missing = wanted.filter((k) => !(k in out))
  if (missing.length) console.error(`no call site for ${missing.length}: ${missing.join(', ')}`)
}
