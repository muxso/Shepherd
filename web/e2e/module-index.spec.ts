import { test, expect } from '@playwright/test'
import { moduleIndex } from '../src/components/moduleIndex'
import type { ApiModule } from '../src/api'
test('indexes thousands of cases once and retains subtree counts', () => {
 const modules = [{ id: 'root', name: 'root', projectId: 'p', parentId: null }, ...Array.from({ length: 750 }, (_, i) => ({ id: `m${i}`, name: `m${i}`, projectId: 'p', parentId: 'root' }))] as ApiModule[]
 const items = Array.from({ length: 8000 }, (_, i) => ({ module: `m${i % 750}` }))
 let calls = 0
 const index = moduleIndex(modules, items, item => { calls++; return item.module })
 expect(calls).toBe(8000)
 expect(index.counts.get('root')).toBe(8000)
 expect(index.counts.get('m0')).toBe(11)
 expect(index.unfiled).toBe(0)
})
