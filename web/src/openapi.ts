// 轻量 OpenAPI 3.x / Swagger 2.0 解析:从 schema 生成示例,把每个操作整理成可建用例的请求。
// 用于「导入即按 schema(必填/选填)自动生成用例」,免去用户手敲 JSON。

type Doc = any

function resolveRef(ref: string, doc: Doc): any {
  // "#/components/schemas/X" 或 "#/definitions/X"
  const parts = ref.replace(/^#\//, '').split('/')
  let cur: any = doc
  for (const p of parts) cur = cur?.[p]
  return cur
}

export function schemaToExample(schema: any, doc: Doc, depth = 0): any {
  if (!schema || depth > 6) return null
  if (schema.$ref) return schemaToExample(resolveRef(schema.$ref, doc), doc, depth + 1)
  if (schema.example !== undefined) return schema.example
  if (schema.default !== undefined) return schema.default
  if (Array.isArray(schema.enum) && schema.enum.length) return schema.enum[0]
  const t = schema.type || (schema.properties ? 'object' : undefined)
  switch (t) {
    case 'object': {
      const out: Record<string, any> = {}
      const props = schema.properties || {}
      for (const k of Object.keys(props)) out[k] = schemaToExample(props[k], doc, depth + 1)
      return out
    }
    case 'array':
      return [schemaToExample(schema.items, doc, depth + 1)].filter((x) => x !== null)
    case 'integer':
    case 'number':
      return 0
    case 'boolean':
      return true
    case 'string':
      if (schema.format === 'date-time') return new Date().toISOString()
      if (schema.format === 'date') return '2024-01-01'
      if (schema.format === 'uuid') return '00000000-0000-0000-0000-000000000000'
      return 'string'
    default:
      return null
  }
}

export interface OpOperation {
  summary?: string
  query: { name: string; required: boolean; example: string }[]
  bodyExample?: string // JSON 字符串
}

// 返回 key=`${METHOD} ${path}` → 操作信息。同时支持 OpenAPI3(requestBody)与 Swagger2(in:body 参数)。
export function parseOperations(doc: Doc): Record<string, OpOperation> {
  const out: Record<string, OpOperation> = {}
  const paths = doc?.paths || {}
  const methods = ['get', 'post', 'put', 'delete', 'patch']
  for (const path of Object.keys(paths)) {
    const pathItem = paths[path] || {}
    for (const m of methods) {
      const op = pathItem[m]
      if (!op) continue
      const params = [...(pathItem.parameters || []), ...(op.parameters || [])].map((p) =>
        p.$ref ? resolveRef(p.$ref, doc) : p,
      )
      const query = params
        .filter((p) => p.in === 'query')
        .map((p) => ({
          name: p.name,
          required: !!p.required,
          example: String(schemaToExample(p.schema || p, doc) ?? ''),
        }))
      // body:OpenAPI3
      let bodySchema: any =
        op.requestBody?.content?.['application/json']?.schema ||
        op.requestBody?.content?.['*/*']?.schema
      // body:Swagger2(in:body 参数)
      if (!bodySchema) bodySchema = params.find((p) => p.in === 'body')?.schema
      let bodyExample: string | undefined
      if (bodySchema) {
        const ex = schemaToExample(bodySchema, doc)
        if (ex !== null && ex !== undefined) bodyExample = JSON.stringify(ex, null, 2)
      }
      out[`${m.toUpperCase()} ${path}`] = { summary: op.summary || op.operationId, query, bodyExample }
    }
  }
  return out
}

// 给定操作信息,构造用于建用例的 url(路径 + 必填 query)。
export function buildCaseUrl(path: string, op?: OpOperation): string {
  if (!op) return path
  const req = op.query.filter((q) => q.required && q.name)
  if (!req.length) return path
  const qs = req.map((q) => `${encodeURIComponent(q.name)}=${encodeURIComponent(q.example)}`).join('&')
  return `${path}${path.includes('?') ? '&' : '?'}${qs}`
}
