import { request } from '../../api'

// External Yunxiao facts are separate from native verified deliveries.
export interface DevopsSource {
  id: string; name: string; project_id: string; cloud_org_id: string; cloud_project_id: string;
  last_success_at?: string | null; code_since?: string | null; code_until?: string | null;
}
export interface DevopsAuthor { name: string; group: string; commits: number; linkedCommits: number; additions: number | null; deletions: number | null; unknownStats: number }
export interface DevopsDashboard {
  source: DevopsSource; totals: { requirements: number; bugs: number; sprints: number; delayed: number; delivered: number };
  authors: DevopsAuthor[]; codeScope: string; historyScope: string;
}
export interface DevopsRow {
  externalId?: string; id?: string | number; serial?: string; title?: string; sha?: string; url?: string;
  additions?: number | null; deletions?: number | null; requirementIds?: string[];
  [key: string]: unknown;
}
export const devopsApi = {
  sources: () => request<{ items: DevopsSource[] }>('GET', '/api/devops/sources'),
  dashboard: (id: string, days: number) => request<DevopsDashboard>('GET', `/api/devops/sources/${encodeURIComponent(id)}/dashboard?days=${days}`),
  records: (id: string, query: Record<string, string | number | boolean>) => request<{ items: DevopsRow[]; total: number }>('GET', `/api/devops/sources/${encodeURIComponent(id)}/records?${new URLSearchParams(Object.entries(query).map(([k, v]) => [k, String(v)]))}`),
}

