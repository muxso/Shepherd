{{/*
Expand the name of the chart.
*/}}
{{- define "shepherd.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Create a fully qualified app name (release-name scoped).
*/}}
{{- define "shepherd.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{/*
Chart name and version label.
*/}}
{{- define "shepherd.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Common labels.
*/}}
{{- define "shepherd.labels" -}}
helm.sh/chart: {{ include "shepherd.chart" . }}
{{ include "shepherd.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: shepherd
{{- end -}}

{{/*
Selector labels (chart-wide).
*/}}
{{- define "shepherd.selectorLabels" -}}
app.kubernetes.io/name: {{ include "shepherd.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/*
Per-component selector labels. Pass a dict: { "root": ., "component": "server" }.
*/}}
{{- define "shepherd.componentSelectorLabels" -}}
{{ include "shepherd.selectorLabels" .root }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{/*
Per-component labels (common labels + component).
*/}}
{{- define "shepherd.componentLabels" -}}
{{ include "shepherd.labels" .root }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{/*
Component fullname, e.g. <fullname>-server.
*/}}
{{- define "shepherd.componentName" -}}
{{- printf "%s-%s" (include "shepherd.fullname" .root) .component | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Image reference for a component. Pass a dict: { "root": ., "repo": "shepherd-server" }.
Uses global.image.{registry,tag,pullPolicy}.
*/}}
{{- define "shepherd.image" -}}
{{- $img := .root.Values.global.image -}}
{{- printf "%s/%s:%s" $img.registry .repo (default "latest" $img.tag) -}}
{{- end -}}

{{/*
Image pull policy.
*/}}
{{- define "shepherd.imagePullPolicy" -}}
{{- default "IfNotPresent" .Values.global.image.pullPolicy -}}
{{- end -}}

{{/*
ServiceAccount name to use.
*/}}
{{- define "shepherd.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "shepherd.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{/*
Server Service name (used by web nginx + agent-runtime SHEPHERD_BASE).
*/}}
{{- define "shepherd.serverServiceName" -}}
{{- printf "%s-server" (include "shepherd.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Shared Secret name.
*/}}
{{- define "shepherd.secretName" -}}
{{- printf "%s-secret" (include "shepherd.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Shared (non-secret) env ConfigMap name.
*/}}
{{- define "shepherd.envConfigMapName" -}}
{{- printf "%s-env" (include "shepherd.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Effective DATABASE_URL: explicit database.url, else derived from the in-cluster
postgresql subchart when enabled, else "".
*/}}
{{- define "shepherd.databaseUrl" -}}
{{- if .Values.database.url -}}
{{- .Values.database.url -}}
{{- else if .Values.postgresql.enabled -}}
{{- $pg := .Values.postgresql.auth -}}
{{- printf "postgres://%s:%s@%s:5432/%s" $pg.username $pg.password (include "shepherd.pgServiceName" .) $pg.database -}}
{{- end -}}
{{- end -}}

{{/*
In-cluster dev Postgres / Redis Service names.
*/}}
{{- define "shepherd.pgServiceName" -}}
{{- printf "%s-postgresql" (include "shepherd.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- define "shepherd.redisServiceName" -}}
{{- printf "%s-redis" (include "shepherd.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Effective fleet Redis URL: explicit config.fleet.redisUrl, else derived from the
in-cluster redis subchart when enabled (auth disabled), else "".
*/}}
{{- define "shepherd.redisUrl" -}}
{{- if .Values.config.fleet.redisUrl -}}
{{- .Values.config.fleet.redisUrl -}}
{{- else if .Values.redis.enabled -}}
{{- printf "redis://%s:6379" (include "shepherd.redisServiceName" .) -}}
{{- end -}}
{{- end -}}
