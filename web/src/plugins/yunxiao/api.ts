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
  groups?: string[];
  statuses?: { kind: string; status: string; count: number }[];
  bugStatistics?: BugStatistics;
  source: DevopsSource; totals: { requirements: number; bugs: number; sprints: number; delayed: number; delivered: number };
  authors: DevopsAuthor[]; codeScope: string; historyScope: string;
}
export interface DevopsRow {
  externalId?: string; id?: string | number; serial?: string; title?: string; sha?: string; url?: string;
  requirements?: { externalId: string; title?: string | null; serial?: string | null }[];
  quality?: { analyses: number; failed: number };
  additions?: number | null; deletions?: number | null; requirementIds?: string[];
  [key: string]: unknown;
}
export interface AuthorIdentity { name: string; group?: string | null; aliases: string[] }
export interface AuthorIdentitiesData { identities: AuthorIdentity[]; permissions: { update: boolean } }
export interface SonarCondition {
  status?: string | null;
  threshold?: string | number | null;
  comparator?: string | null;
}
export interface SonarAnalysis {
  scope: 'sonar_repository' | 'ci_pull_request';
  taskId: string;
  projectKey?: string;
  repositoryId: string;
  repositoryName?: string;
  commitAuthor?: string;
  personName?: string;
  repositoryUrl?: string;
  sonarUrl?: string;
  branch?: string;
  pullRequest: string | null;
  revision: string;
  author: string | null;
  analyzedAt: string;
  gate: 'OK' | 'ERROR' | 'WARN' | 'NONE';
  newIssues: number | null;
  newCoverage: number | null;
  newDuplication: number | null;
  push?: { baseRevision: string; branch: string; pushedAt: string; author: string | null; baselineVerified?: boolean };
  analysisQuality?: {
    newIssues?: number | null; acceptedIssues?: number | null; coverage?: number | null;
    linesToCover?: number | null; duplication?: number | null; lines?: number | null;
    securityHotspots?: number | null;
    conditions?: Partial<Record<'new_violations' | 'new_coverage' | 'new_duplicated_lines_density', SonarCondition>>;
  };
  overallQuality?: {
    issues?: number; acceptedIssues?: number; coverage?: number;
    duplication?: number; securityHotspots?: number;
  };
  newCode?: {
    lines?: number | null; securityIssues?: number | null; securityRating?: number | null;
    reliabilityIssues?: number | null; reliabilityRating?: number | null;
    maintainabilityIssues?: number | null; maintainabilityRating?: number | null;
    acceptedIssues?: number | null; securityHotspots?: number | null; linesToCover?: number | null; duplicatedLines?: number | null;
  };
}
export interface StatusDwellStage {
  status: string;
  phase: 'completed' | 'current';
  enteredAt: string;
  exitedAt: string | null;
  seconds: number;
}
export interface StatusDwellData {
  asOf: string | null;
  month: string | null;
  total: number;
  stages: string[];
  currentStatusCounts: Record<string, number>;
  workItems: {
    externalId: string;
    serial: string | null;
    title: string | null;
    currentStatus: string | null;
    seconds: number | null;
    stages: StatusDwellStage[];
  }[];
}

export const devopsApi = {

  authorIdentities: (id: string) => request<AuthorIdentitiesData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/author-identities`),
  setAuthorIdentities: (id: string, identities: AuthorIdentity[]) => request<{ identities: AuthorIdentity[] }>('PUT', `/api/devops/sources/${encodeURIComponent(id)}/author-identities`, { identities }),
  codeAuthors: (id: string, month: string) => request<CodeAuthorsData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/code-authors?${new URLSearchParams({ month })}`),
  memberMetrics: (id: string, month: string, requirementTarget: number) => request<MemberMetricsData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/member-metrics?${new URLSearchParams({ month, requirementTarget: String(requirementTarget) })}`),
  requirementOutput: (query: Record<string, string | number>) => request<RequirementOutputData>('GET', `/api/devops/requirement-output?${new URLSearchParams(Object.entries(query).map(([key,value]) => [key,String(value)]))}`),
  bugStatistics: (id: string, query: Record<string,string>) => request<FilteredBugStatistics>('GET', `/api/devops/sources/${encodeURIComponent(id)}/bug-statistics?${new URLSearchParams(query)}`),
  sources: () => request<{ items: DevopsSource[] }>('GET', '/api/devops/sources'),
  dashboard: (id: string, days: number) => request<DevopsDashboard>('GET', `/api/devops/sources/${encodeURIComponent(id)}/dashboard?days=${days}`),
  statusDwell: (id: string, kind: 'requirement' | 'bug', month: string, search: string, page: number, status: string, sortOrder: 'asc' | 'desc', currentStatus: string) => request<StatusDwellData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/records?${new URLSearchParams({ kind: `status-dwell-${kind}`, month, search, page: String(page), pageSize: '15', status, sortOrder, currentStatus })}`),
  records: (id: string, query: Record<string, string | number | boolean>) => request<{ items: DevopsRow[]; total: number }>('GET', `/api/devops/sources/${encodeURIComponent(id)}/records?${new URLSearchParams(Object.entries(query).map(([k, v]) => [k, String(v)]))}`),
}


export interface BugStatistics {
  total: number; new: number; linked: number; unlinked: number;
  statuses: { status: string; count: number }[]; reporters: { name: string; count: number }[];
}
export interface FilteredBugStatistics extends BugStatistics {
  source: DevopsSource; from: string; through: string; timezone: string; unassigned: number; missingTime: number;
  people: { name: string; count: number }[]; personOptions: string[]; groups: string[];
  trend: { date: string; value: number }[]; priorities: { name: string; count: number }[];
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
  onTimeRate?: number | null;
  sonar?: { pushes: number; passed: number; evaluated: number; passRate: number | null; recent: SonarAnalysis[] } | null;
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
export interface SonarActivityDay { member: string; date: string; pushes: number; passed: number; failed: number; unverifiedBaseline?: number; recent?: SonarAnalysis[] }
export interface RegressionActivityDay { member: string; date: string; passed: number; evaluated: number }
export interface TeamActivityData {
  source: DevopsSource; role: ActivityRole; days: number; from: string; through: string; start: string; endExclusive: string; timezone: string; observedAt: string | null;
  members: ActivityMember[]; events: ActivityEvent[]; bugs: ActivityBug[]; reviewPolicy?: ActivityReviewPolicy;
  sonarDaily?: SonarActivityDay[];
  regressionDaily?: RegressionActivityDay[];
  coverage: { complete: boolean; rosterConfigured: boolean; unattributedEvents: number; testcasesCollected: boolean };
  permissions: { requirements: boolean; bugs: boolean; testcases: boolean };
}
export interface ReviewRule { enabled: boolean; inactiveDays: number; activeWindowDays: number; minActiveDays: number }
export type ActivityReviewPolicy = Record<ActivityRole, ReviewRule>
export const activityApi = {
  reviewPolicy: (id: string) => request<ActivityReviewPolicy & { permissions: { update: boolean } }>('GET', `/api/devops/sources/${encodeURIComponent(id)}/activity-review-policy`),
  saveReviewPolicy: (id: string, policy: ActivityReviewPolicy) => request<ActivityReviewPolicy>('PUT', `/api/devops/sources/${encodeURIComponent(id)}/activity-review-policy`, policy),
  activity: (id: string, role: ActivityRole, days: number) => request<TeamActivityData>('GET', `/api/devops/sources/${encodeURIComponent(id)}/team-activity?${new URLSearchParams({ role, days: String(days) })}`),
  regressionActivity: (days: number) => request<RegressionActivityDay[]>('GET', `/api/scenario/regression-activity?days=${days}`),
  members: (id: string) => request<{ members: ActivityMemberSetting[]; permissions: { update: boolean } }>('GET', `/api/devops/sources/${encodeURIComponent(id)}/activity-members`),
  saveMembers: (id: string, members: ActivityMemberSetting[]) => request<{ members: ActivityMemberSetting[] }>('PUT', `/api/devops/sources/${encodeURIComponent(id)}/activity-members`, { members }),
}
