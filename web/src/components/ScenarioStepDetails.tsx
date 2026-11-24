import { Badge } from 'antd'
import { DownOutlined } from '@ant-design/icons'
import { fmtDuration } from './ScenarioReport'
import type { ApiCase, ReportResultItem, ScenarioStep } from '../api'
import type { Profile } from '../plugins/environment/api'
import { splitProcessors } from '../plugins/environment/preparation'

type Request = Pick<ApiCase, 'method' | 'url' | 'body' | 'assertions' | 'processors'> | NonNullable<ScenarioStep['request']>
type Item = { type?: string; args?: Record<string, unknown> }
type Preparation = { label: string; source?: string; variable?: string; query?: string }

const items = (value: unknown): Item[] => Array.isArray(value) ? value.filter(item => item && typeof item === 'object') : []
const text = (value: unknown): string => value == null ? '' : typeof value === 'string' ? value : JSON.stringify(value)

function preparation(request: Request, profiles: Record<string, Profile>): Preparation[] {
  const output: Preparation[] = []
  for (const item of items(splitProcessors(items(request.processors)).preProcessors)) {
    const args = item.args ?? {}
    if (item.type === 'Sql') {
      const source = text(args.datasource)
      output.push({ label: 'SQL 前置', source: profiles[source]?.name ?? source, variable: text(args.name), query: text(args.sql) })
      continue
    }
    if (item.type !== 'Plugin') {
      output.push({ label: item.type === 'Script' ? 'JavaScript' : item.type === 'Wait' ? '等待' : text(item.type), source: item.type === 'Wait' ? `${text(args.ms)} ms` : undefined })
      continue
    }
    const id = text(args.profileId)
    const profile = profiles[id]
    const steps = profile?.config.values.steps
    const stages = Array.isArray(steps) ? steps.map(step => {
      const query = step?.input?.query
      const source = text(step?.profileId)
      const name = profiles[source]?.name ?? source
      if (step?.type === 'invoke' && typeof query === 'string') {
        return { label: 'SQL 前置', source: name, variable: text(step.resultVariable), query }
      }
      return { label: step?.type === 'wait' ? '等待' : text(step?.type) || '前置插件', source: name }
    }) : []
    output.push(...(stages.length ? stages : [{ label: '前置插件', source: profile?.name ?? id }]))
  }
  return output
}

export function stepDetails(request: Request, profiles: Record<string, Profile>, result?: ReportResultItem) {
  const pre = preparation(request, profiles)
  const assertions = items(request.assertions)
  const checks = result?.assertions ?? []
  return {
    badge: pre.some(item => item.label === 'SQL 前置') ? 'SQL 前置' : pre.length ? '前置处理' : undefined,
    content: <div className="ms-step-details">
      {pre.length > 0 && <section>
        <h4>前置处理</h4>
        <div className="ms-step-detail-stages">
          {pre.map(({ label, source, variable, query }, index) => {
            const stage = <><strong>{label}</strong>{source && <span>{source}</span>}{variable && <code>{variable}</code>}</>
            return query ? <details key={index}>
              <summary><DownOutlined aria-hidden="true" />{stage}</summary>
              <pre>{query}</pre>
            </details> : <div key={index}>{stage}</div>
          })}
        </div>
      </section>}
      <section>
        <h4>HTTP 请求</h4>
        <div>
          <div className="ms-step-detail-request"><strong>{request.method}</strong><code>{request.url}</code></div>
          {request.body && <pre>{request.body}</pre>}
        </div>
      </section>
      <section>
        <h4>断言</h4>
        <div className="ms-step-detail-checks">
          {result && <div className="ms-step-detail-meta">
            <Badge status={result.outcome === 'SUCCESS' ? 'success' : 'error'} text={result.outcome === 'SUCCESS' ? '最近一次通过' : '最近一次失败'} />
            {result.statusCode != null && <code>HTTP {result.statusCode}</code>}
            {result.latencyMs != null && <span>响应耗时 {fmtDuration(result.latencyMs)}</span>}
            {result.executedAt && <time dateTime={result.executedAt}>{new Date(result.executedAt).toLocaleString()}</time>}
          </div>}
          {checks.length > 0 || assertions.length > 0 ? <div className="ms-step-detail-table">
            <table>
              <colgroup>
                <col className="ms-step-check-item" /><col />
                {checks.length > 0 && <><col /><col className="ms-step-check-result" /></>}
              </colgroup>
              <thead><tr><th scope="col">检查项</th><th scope="col">预期</th>{checks.length > 0 && <><th scope="col">实际</th><th scope="col" className="ms-step-check-result">结果</th></>}</tr></thead>
              <tbody>{checks.length > 0
                ? checks.map(({ item, expected, actual, passed }, index) => <tr key={index}>
                  <td>{item}</td><td><code>{expected}</code></td><td><code>{actual}</code></td>
                  <td className="ms-step-check-result"><Badge status={passed ? 'success' : 'error'} text={passed ? '通过' : '失败'} /></td>
                </tr>)
                : assertions.map(({ type, args }, index) => <tr key={index}>
                  <td>{text(args?.pointer ?? args?.path ?? type)}</td><td><code>{text(args?.expected ?? args)}</code></td>
                </tr>)}</tbody>
            </table>
          </div> : <span className="ms-step-detail-empty">无断言</span>}
        </div>
      </section>
    </div>,
  }
}
