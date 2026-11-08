import { request } from '../../api'
export interface CodeRepository {id:string;name:string;url:string;branch:string;commitSha:string;files:number;updatedAt:string}
export interface RepositoryBinding { id:string;name:string;branch:string;url:string;commitSha:string }
export interface TestService { id:string;project_id:string;name:string;description:string;repositories:RepositoryBinding[];updated_at:string }
export interface CaseAsset { id:string;name:string;module:string;serviceId:string|null;sourceId:string|null;externalId:string|null;libraryId:string;library:string;directory:string;sourceAvailable:boolean|null;sourceRevision:string|null }
export interface SourceMapping {id:string;source_id:string;scope:'case'|'directory'|'library';external_id:string;service_id:string}
export interface FunctionalScenarioStep {caseId:string;instruction:string;expected:string;parameters:string}
export interface FunctionalScenario {id:string;name:string;description:string;preconditions:string;steps:FunctionalScenarioStep[];requirement_ids:string[];api_scenario_id:string|null;revision:number;stale?:boolean;status:'DRAFT'|'REVIEWED';updated_at:string}
export interface ScenarioRun {id:string;revision:number;environment:string;outcome:string;notes:string;steps:{actual:string;outcome:string}[];executed_by:string;executed_at:string}
export interface TestInventory {id:string;service_id:string;repository_id:string;branch:string;commit_sha:string;state:string;endpoints:InventoryEndpoint[];limitations:string[];scanned_paths:string[];created_at:string}
export interface InventoryEndpoint {method:string;path:string;file:string;line:number;handler?:string;apiDefinitionId?:string;apiCaseIds?:string[];dependencies?:string[]}
export interface ImportItem {kind:string;externalId:string;nativeId?:string;action:string;title:string;current:unknown;target:unknown;deleted?:boolean;archived?:boolean}
export interface ImportBatch {id:string;source_id:string;source_revision:string;status:string;created_at:string;preview:{counts:Record<string,number>;items:ImportItem[];manualAssets:number};result?:{imported:number;conflicts:number;unchanged:number;sourceMissing:number}}
export interface CaseApiLink {id:string;case_id:string;api_definition_id:string|null;api_case_id:string|null}
export interface TestAssets {apiLinks?:CaseApiLink[];services:TestService[];cases:CaseAsset[];mappings:SourceMapping[];scenarios:FunctionalScenario[];inventories:TestInventory[];imports:ImportBatch[];permissions:{edit:boolean;manage:boolean;import:boolean}}
const root=(p:string)=>`/test-assets/projects/${encodeURIComponent(p)}`
export const assetApi={
 caseApi:(p:string,id:string,links:{apiDefinitionId:string|null;apiCaseId:string|null}[])=>request('PUT',root(p)+'/cases/'+encodeURIComponent(id)+'/api-links',{links}),
 repositories:(p:string)=>request<{items:CodeRepository[]}>('GET','/rag/code/repositories?'+new URLSearchParams({projectId:p})),
 scan:(p:string,serviceId:string,repositoryId:string,branch:string)=>request<TestInventory>('POST','/rag/code/repositories/'+encodeURIComponent(repositoryId)+'/inventory',{projectId:p,serviceId,branch}),
 overview:(p:string)=>request<TestAssets>('GET',root(p)),
 sources:(p:string)=>request<{items:{id:string;name:string;last_success_at?:string}[];enabled?:boolean}>('GET',root(p)+'/sources'),
 saveService:(p:string,id:string|undefined,b:{name:string;description:string;repositories:RepositoryBinding[]})=>request<TestService>(id?'PUT':'POST',root(p)+'/services'+(id?'/'+encodeURIComponent(id):''),b),
 deleteService:(p:string,id:string)=>request('DELETE',root(p)+'/services/'+encodeURIComponent(id)),
 assign:(p:string,caseIds:string[],serviceId:string|null)=>request('PUT',root(p)+'/case-services',{caseIds,serviceId}),
 mapping:(p:string,b:{sourceId:string;scope:string;externalId:string;serviceId:string})=>request('PUT',root(p)+'/mappings',b),
 preview:(p:string,sourceId:string)=>request<ImportBatch>('POST',root(p)+'/imports',{sourceId}),
 importItem:(p:string,id:string,kind:string,externalId:string)=>request<ImportItem>('GET',root(p)+'/imports/'+encodeURIComponent(id)+'/items?'+new URLSearchParams({kind,externalId})),
 apply:(p:string,id:string,resolutions:Record<string,string>={})=>request<{imported:number;conflicts:number;unchanged:number}>('POST',root(p)+'/imports/'+encodeURIComponent(id)+'/apply',{resolutions}),
 saveScenario:(p:string,id:string|undefined,b:{name:string;description:string;preconditions:string;steps:FunctionalScenarioStep[];requirementIds:string[];apiScenarioId:string|null;status:string})=>request<FunctionalScenario>(id?'PUT':'POST',root(p)+'/scenarios'+(id?'/'+encodeURIComponent(id):''),b),
 deleteScenario:(p:string,id:string)=>request('DELETE',root(p)+'/scenarios/'+encodeURIComponent(id)),
 runs:(p:string,id:string)=>request<{items:ScenarioRun[]}>('GET',root(p)+'/scenarios/'+encodeURIComponent(id)+'/runs'),
 recordRun:(p:string,id:string,b:{revision:number;environment:string;notes:string;steps:{actual:string;outcome:string}[]})=>request<ScenarioRun>('POST',root(p)+'/scenarios/'+encodeURIComponent(id)+'/runs',b),
 inventory:(p:string,id:string,b:{baseInventoryId?:string;repositoryId:string;branch:string;commitSha:string;state:string;endpoints:InventoryEndpoint[];limitations:string[];scannedPaths:string[]})=>request<TestInventory>('POST',root(p)+'/services/'+encodeURIComponent(id)+'/inventories',b),
}
