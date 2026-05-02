{{- /*
taskchampion-sync-server helpers
*/ -}}

{{- define "taskchampion-sync-server.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "taskchampion-sync-server.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "taskchampion-sync-server.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
app.kubernetes.io/name: {{ include "taskchampion-sync-server.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "taskchampion-sync-server.selectorLabels" -}}
app.kubernetes.io/name: {{ include "taskchampion-sync-server.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "taskchampion-sync-server.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "taskchampion-sync-server.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{- define "taskchampion-sync-server.postgres-connection" -}}
  {{- $host := .Values.postgres.host -}}
  {{- $port := .Values.postgres.port -}}
  {{- $username := .Values.postgres.username -}}
  {{- $password := .Values.postgres.password -}}
  {{- $database := .Values.postgres.database -}}
  
  {{- /* Build URI */ -}}
  {{- $uri := printf "postgresql://" -}}
  {{- if ne $username "" -}}
    {{- $uri = printf "%s%s" $uri $username -}}
    {{- if ne $password "" -}}
      {{- $uri = printf "%s:%s" $uri $password -}}
    {{- end -}}
    {{- $uri = printf "%s@" $uri -}}
  {{- end -}}
  {{- $uri = printf "%s%s" $uri $host -}}
  {{- if ne (printf "%v" $port) "5432" -}}
    {{- $uri = printf "%s:%v" $uri $port -}}
  {{- end -}}
  {{- if ne $database "taskchampion" -}}
    {{- $uri = printf "%s/%s" $uri $database -}}
  {{- end -}}
  {{- $uri -}}
{{- end -}}

{{- define "taskchampion-sync-server.schema-url" -}}
{{- if .Values.postgres.initContainer.schemaUrl -}}
{{- .Values.postgres.initContainer.schemaUrl -}}
{{- else -}}
{{- printf "https://raw.githubusercontent.com/GothenburgBitFactory/taskchampion-sync-server/v%s/postgres/schema.sql" .Chart.AppVersion -}}
{{- end -}}
{{- end -}}

{{- define "taskchampion-sync-server.postgres-secret-name" -}}
{{- if .Values.postgres.existingSecret -}}
{{- .Values.postgres.existingSecret -}}
{{- else -}}
{{- printf "%s-connection" .Release.Name -}}
{{- end -}}
{{- end -}}