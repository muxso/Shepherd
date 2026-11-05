// Reusable flows across pages and APIs.
import { RequirementsPage } from './pages/RequirementsPage'

/** Open a requirement and decompose it into the task graph. */
export async function decompose(reqs: RequirementsPage, title: string) {
  await reqs.goto()
  await reqs.open(title)
  await reqs.breakdown()
}
