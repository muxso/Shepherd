// 跨页面/接口的可复用业务流 —— spec 靠组合 flow + page-object 表达,不复制粘贴步骤。
import { RequirementsPage } from './pages/RequirementsPage'

/** 列表打开某需求 → 点「自动拆分」,落到拆分视图(decompId 已记)。 */
export async function decompose(reqs: RequirementsPage, title: string) {
  await reqs.goto()
  await reqs.open(title)
  await reqs.breakdown()
}
