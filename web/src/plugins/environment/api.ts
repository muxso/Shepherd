import { request, ApiError } from '../../api'

export type FieldType =
  | { type: 'string'; max_length: number }
  | { type: 'integer'; min: number; max: number }
  | { type: 'boolean' }
  | { type: 'choice'; options: string[] }
  | { type: 'string_list'; max_items: number }
  | { type: 'json' }
  | { type: 'secret' }
export interface ConfigField { name: string; label: string; required: boolean; kind: FieldType }
export interface Descriptor {
  id: string; name: string; section: string; version: string; apiVersion: number
  capabilities: string[]; operations: string[]; configSchema: { version: number; fields: ConfigField[] }
}
export interface PluginConfig { schemaVersion: number; values: Record<string, unknown>; secrets: Record<string, string> }
export interface NewProfile { pluginId: string; name: string; config: PluginConfig; enabled: boolean }
export interface Profile extends NewProfile { id: string; projectId: string; revision: number }
export type Slot = 'connection' | 'pre' | 'post' | 'assert' | 'host' | 'display' | 'cleanup'
export interface Binding { profileId: string; slot: Slot; priority: number; enabled: boolean }
export interface Credential { id: string; label: string; environmentId: string | null }
export interface Configuration { profiles: Profile[]; bindings: Binding[]; credentials: Credential[]; permissions: { manage: boolean; execute: boolean } }
export interface PluginOutput<T = unknown> { status: 'success' | 'failure'; data: T; variables: Record<string, string>; assertions: { id: string; passed: boolean; message: string }[] }
const root = (environment: string) => `/api/environment-plugins/environments/${encodeURIComponent(environment)}`
const profilePath = (environment: string, id: string) => `${root(environment)}/profiles/${encodeURIComponent(id)}`
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  try { return await request<T>(method, path, body) }
  catch (error) {
    if (error instanceof ApiError) {
      let message: unknown
      try { message = (JSON.parse(error.message) as { error?: unknown }).error } catch { /* plain error */ }
      if (typeof message === 'string') throw new ApiError(error.status, message)
    }
    throw error
  }
}
export const environmentPluginApi = {
  capabilities: () => call<{ installed?: boolean; enabled: boolean; items: Descriptor[] }>('GET', '/api/environment-plugins/capabilities'),
  configuration: (environment: string) => call<Configuration>('GET', root(environment)),
  create: (environment: string, profile: NewProfile) => call<Profile>('POST', `${root(environment)}/profiles`, profile),
  update: (environment: string, id: string, revision: number, profile: NewProfile) => call<Profile>('PUT', profilePath(environment, id), { revision, profile }),
  remove: (environment: string, id: string, revision: number) => call('DELETE', `${profilePath(environment, id)}?revision=${revision}`),
  bind: (environment: string, id: string, slot: Slot, enabled = true) => call('PUT', `${root(environment)}/bindings/${encodeURIComponent(id)}`, { slot, priority: 0, enabled }),
  unbind: (environment: string, id: string, slot: Slot) => call('DELETE', `${root(environment)}/bindings/${encodeURIComponent(id)}?slot=${slot}`),
  test: (environment: string, id: string) => call<PluginOutput>('POST', `${profilePath(environment, id)}/test`),
  execute: (environment: string, id: string, input: unknown) => call<PluginOutput<DatabaseReport>>('POST', `${profilePath(environment, id)}/execute`, input),
  createCredential: (environment: string, label: string, value: string) => call<{ reference: string }>('POST', `${root(environment)}/credentials`, { label, value, environmentOnly: true }),
}

export interface DatabaseReport {
  columns: { name: string; sqlType: string }[]
  rows: unknown[][]
  rowsAffected: number
  truncated: boolean
  elapsedMs: number
}
