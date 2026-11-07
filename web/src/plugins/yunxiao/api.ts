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
export interface AuthorIdentity { name: string; group?: string | null; aliases: string[] }
export interface AuthorIdentitiesData { identities: AuthorIdentity[]; permissions: { update: boolean } }
export const devopsApi = {
  authorIdentities: (id: string) => request<AuthorIdentitiesData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/author-identities`),
  setAuthorIdentities: (id: string, identities: AuthorIdentity[]) => request<{ identities: AuthorIdentity[] }>('PUT', `/api/devops/sources/${encodeURIComponent(id)}/author-identities`, { identities }),
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
  delivered: boolean; currentDelivered?: boolean; newInMonth: boolean; deliveryClass: string; bugIds: string[];
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
  deliveredDueRequirements?: number; dueRequirements: number; onTime: number; deliveredLate: number; overdueAtEnd: number;
  unknownDelivery: number; delayRate: number | null; radar: [number | null, number | null, number | null];
  requirements: MemberRequirementMetric[];
}
export interface BugQualityData {
 totalSourceBugs:number; verifiedSourceBugs:number; unknownSourceBugs:number; processedBugs:number; reopenedBugs:number; reopenCount:number; reopenRate:number | null; rows:MemberBugMetric[];
}
export type ComparisonMember = Pick<MemberMetric,'name' | 'deliveredRequirements' | 'linkedBugs' | 'bugRate' | 'delayRate' | 'reopenCount' | 'reopenUnknownBugs' | 'reopenRate'>
export interface ComparisonPeriod { month:string; start:string; endExclusive:string; members:ComparisonMember[] }
export interface MonthlyComparisons { sameElapsedTime:boolean; previousMonth:ComparisonPeriod | null; previousYear:ComparisonPeriod | null }
export interface MemberMetricsData {
  source: DevopsSource;
  metrics: {
    month: string; start: string; endExclusive: string; timezone: string; requirementTarget: number;
    comparisons?:MonthlyComparisons; bugQuality:BugQualityData; members: MemberMetric[]; newValidBugs: number; linkedValidBugs: number;
    requirementsMissingDue: number; requirementsMissingTime: number; snapshotBased: boolean;
  };
}

export interface CodeCommit {
  id: string; sha: string; author: string; authorRaw?: string; group: string; repoId: string; repo: string;
  authoredAt: string; additions: number | null; deletions: number | null; linked: boolean;
  requirements: { externalId: string; serial?: string | null; title?: string | null }[];
  languages: string[]; languageSource?: string | null; languageCommit?: string | null;
}
export interface CodeAuthorsData {
  source: DevopsSource; month: string; start: string; endExclusive: string; timezone: string;
  commits: CodeCommit[]; permissions: { requirements: boolean };
}

export type ActivityRole = 'code' | 'product' | 'test'
export type ActivityKind = 'commit' | 'requirementCreated' | 'requirementDelivered' | 'testcaseCreated' | 'bugReported'
export interface ActivityMemberSetting { name: string; group?: string | null; roles: ActivityRole[]; enabled: boolean }
export interface ActivityMember { displayName?: string; attribution?: 'person' | 'unmappedAccount' | 'unknownAuthor'; unknownActivityTimes?: boolean; name: string; group: string; inRoster: boolean; eventCount: number; lastActivityAt: string | null; status: 'active' | 'noRecords' | 'unverified' }
export interface ActivityEvent {
  id: string; externalId: string; actor: string; authorRaw: string; group: string; kind: ActivityKind; date: string; at: string;
  branches?: string[] | null; url?: string | null; repositoryUrl?: string | null; title: string | null; serial: string | null; status: string | null; repoId: string | number | null; repo: string | null; sha: string | null;
  additions: number | null; deletions: number | null; linked: boolean; requirements: { externalId: string; serial: string | null; title: string | null; url?: string | null }[];
}
export interface ActivityBug { id: string; serial: string | null; title: string | null; status: string | null; at: string; date: string; responsibles: string[]; requirements: { externalId: string; serial: string | null; title: string | null; url?: string | null }[]; url?: string | null }
export interface TeamActivityData {
  source: DevopsSource; role: ActivityRole; days: number; from: string; through: string; start: string; endExclusive: string; timezone: string; observedAt: string | null;
  members: ActivityMember[]; events: ActivityEvent[]; bugs: ActivityBug[]; reviewPolicy?: ActivityReviewPolicy;
  coverage: { complete: boolean; rosterConfigured: boolean; unattributedEvents: number; testcasesCollected: boolean };
  permissions: { requirements: boolean; bugs: boolean; testcases: boolean };
}
export interface ReviewRule { enabled: boolean; inactiveDays: number; activeWindowDays: number; minActiveDays: number }
export type ActivityReviewPolicy = Record<ActivityRole, ReviewRule>
export const activityApi = {
  reviewPolicy: (id: string) => request<ActivityReviewPolicy & { permissions: { update: boolean } }>('GET', `/api/devops/sources/${encodeURIComponent(id)}/activity-review-policy`),
  saveReviewPolicy: (id: string, policy: ActivityReviewPolicy) => request<ActivityReviewPolicy>('PUT', `/api/devops/sources/${encodeURIComponent(id)}/activity-review-policy`, policy),
  activity: (id: string, role: ActivityRole, days: number) => request<TeamActivityData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/team-activity?${new URLSearchParams({ role, days: String(days) })}`),
  members: (id: string) => request<{ members: ActivityMemberSetting[]; permissions: { update: boolean } }>('GET', `/api/devops/sources/${encodeURIComponent(id)}/activity-members`),
  saveMembers: (id: string, members: ActivityMemberSetting[]) => request<{ members: ActivityMemberSetting[] }>('PUT', `/api/devops/sources/${encodeURIComponent(id)}/activity-members`, { members }),
}
