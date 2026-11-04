import { request } from '../../api'

// External Yunxiao facts are separate from native verified deliveries.
export interface DevopsSource {
  id: string; name: string; project_id: string; cloud_org_id: string; cloud_project_id: string;
  last_success_at?: string | null; code_since?: string | null; code_until?: string | null;
}
export interface DevopsAuthor { name: string; group: string; commits: number; linkedCommits: number; additions: number | null; deletions: number | null; unknownStats: number }
export interface OutputMetric {
  current: number; previous: number; missingTime: number; unattributed: number;
  trend: { date: string; value: number }[]; contributors: { name: string; count: number }[];
}
export interface ProductivityStats {
  from: string; through: string; timezone: string; metrics: Record<string, OutputMetric>;
}
export interface DevopsDashboard {
  productivity?: ProductivityStats;
  bugStatistics?: BugStatistics;
  source: DevopsSource; totals: { requirements: number; bugs: number; sprints: number; delayed: number; delivered: number };
  authors: DevopsAuthor[]; codeScope: string; historyScope: string;
}
export interface DevopsRow {
  externalId?: string; id?: string | number; serial?: string; title?: string; sha?: string; url?: string;
  requirements?: { externalId: string; title?: string | null; serial?: string | null }[];
  additions?: number | null; deletions?: number | null; requirementIds?: string[];
  [key: string]: unknown;
}
export const devopsApi = {
  codeAuthors: (id: string, month: string) => request<CodeAuthorsData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/code-authors?${new URLSearchParams({ month })}`),
  memberMetrics: (id: string, month: string, requirementTarget: number) => request<MemberMetricsData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/member-metrics?${new URLSearchParams({ month, requirementTarget: String(requirementTarget) })}`),
  requirementOutput: (query: Record<string, string | number>) => request<RequirementOutputData>('GET', `/api/devops/requirement-output?${new URLSearchParams(Object.entries(query).map(([key,value]) => [key,String(value)]))}`),
  sources: () => request<{ items: DevopsSource[] }>('GET', '/api/devops/sources'),
  dashboard: (id: string, days: number) => request<DevopsDashboard>('GET', `/api/devops/sources/${encodeURIComponent(id)}/dashboard?days=${days}`),
  records: (id: string, query: Record<string, string | number | boolean>) => request<{ items: DevopsRow[]; total: number }>('GET', `/api/devops/sources/${encodeURIComponent(id)}/records?${new URLSearchParams(Object.entries(query).map(([k, v]) => [k, String(v)]))}`),
}


export interface BugStatistics {
  total: number; new: number; linked: number; unlinked: number;
  statuses: { status: string; count: number }[]; reporters: { name: string; count: number }[];
}
export interface RequirementOutputData {
  source: DevopsSource;
  requirement: DevopsRow & { creator?: string; owner?: string; status?: string };
  output: {
    bugTotal?: number; commitTotal: number; bugs?: DevopsRow[]; commits: DevopsRow[];
    testers?: { name: string; bugs: number }[]; developers: { name: string; commits: number }[];
    bugStates?: { status: string; count: number }[];
    repositories: { id: string; name: string; commits: number; additions: number | null; deletions: number | null; unknownStats: number; authors: string[] }[];
  };
  permissions: { bugs: boolean }; pageSize: number; bugPage: number; commitPage: number;
}

export interface MemberRequirementMetric {
  externalId: string; serial: string; title: string; status: string; due: string;
  delivered: boolean; newInMonth: boolean; deliveryClass: string; bugIds: string[];
}
export interface MemberBugMetric {
  externalId:string; serial:string; title:string; status:string; reopenCount:number | null; reopenTimes:string[];
  createdAt:string; bugType:string; responsibles:string[]; developers:string[]; testers:string[]; reporter:string;
  processed:boolean; historyAvailable:boolean; history:{id:string;at:string;from:string;to:string}[];
}
export interface MemberMetric {
  name: string; group: string; newRequirements: number; deliveredRequirements: number;
  linkedBugs: number; bugAffectedRequirements: number; bugRate: number | null;
  processedBugs:number; qualityBugs:number; reopenUnknownBugs:number;
  reopenCount:number; reopenedBugs:number; reopenEvaluatedBugs:number; reopenRate:number | null; bugs:MemberBugMetric[];
  dueRequirements: number; onTime: number; deliveredLate: number; overdueAtEnd: number;
  unknownDelivery: number; delayRate: number | null; radar: [number | null, number | null, number | null];
  requirements: MemberRequirementMetric[];
}
export interface BugQualityData {
 totalSourceBugs:number; verifiedSourceBugs:number; unknownSourceBugs:number; processedBugs:number; reopenedBugs:number; reopenCount:number; reopenRate:number | null; rows:MemberBugMetric[];
}
export interface MemberMetricsData {
  source: DevopsSource;
  metrics: {
    month: string; start: string; endExclusive: string; timezone: string; requirementTarget: number;
    bugQuality:BugQualityData; members: MemberMetric[]; newValidBugs: number; linkedValidBugs: number;
    requirementsMissingDue: number; requirementsMissingTime: number; snapshotBased: boolean;
  };
}

export interface CodeCommit {
  id: string; sha: string; author: string; group: string; repoId: string; repo: string;
  authoredAt: string; additions: number | null; deletions: number | null; linked: boolean;
  requirements: { externalId: string; serial?: string | null; title?: string | null }[];
  languages: string[]; languageSource?: string | null; languageCommit?: string | null;
}
export interface CodeAuthorsData {
  source: DevopsSource; month: string; start: string; endExclusive: string; timezone: string;
  commits: CodeCommit[]; permissions: { requirements: boolean };
}
