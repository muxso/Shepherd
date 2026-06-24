// Scan src for literal t('key') usages and report keys missing from DICT in i18n.tsx.
import { readFileSync, readdirSync, statSync } from 'fs'
import { join } from 'path'

const SRC = new URL('../src', import.meta.url).pathname
const i18n = readFileSync(join(SRC, 'i18n.tsx'), 'utf8')

// DICT keys: lines like  'key': { zh: ... }  or "key": { ...
const dictKeys = new Set()
for (const m of i18n.matchAll(/^\s*['"]([a-zA-Z0-9_.]+)['"]\s*:\s*\{\s*zh:/gm)) dictKeys.add(m[1])

function walk(dir) {
  let files = []
  for (const f of readdirSync(dir)) {
    const p = join(dir, f)
    if (statSync(p).isDirectory()) files = files.concat(walk(p))
    else if (/\.(tsx?|ts)$/.test(f)) files.push(p)
  }
  return files
}

const used = new Map() // key -> Set(files)
for (const file of walk(SRC)) {
  if (file.endsWith('i18n.tsx')) continue
  const txt = readFileSync(file, 'utf8')
  for (const m of txt.matchAll(/\bt\(\s*['"]([a-zA-Z0-9_.]+)['"]/g)) {
    if (!used.has(m[1])) used.set(m[1], new Set())
    used.get(m[1]).add(file.replace(SRC + '/', ''))
  }
}

const missing = [...used.keys()].filter((k) => !dictKeys.has(k)).sort()
console.log('DICT keys:', dictKeys.size, '| distinct used literal keys:', used.size, '| MISSING:', missing.length)
for (const k of missing) console.log(k, '  <-', [...used.get(k)].join(', '))
