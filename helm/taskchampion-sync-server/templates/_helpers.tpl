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
  {{- $port := .Values.postgres.port | toString -}}
  {{- $username := .Values.postgres.username -}}
  {{- $password := .Values.postgres.password -}}
  {{- $database := .Values.postgres.database -}}

  {{- /* Override individual fields from existingSecret where present */ -}}
  {{- if .Values.postgres.existingSecret -}}
    {{- $secret := lookup "v1" "Secret" .Release.Namespace .Values.postgres.existingSecret -}}
    {{- if $secret -}}
      {{- if index $secret.data "host" -}}
        {{- $host = index $secret.data "host" | b64dec -}}
      {{- end -}}
      {{- if index $secret.data "port" -}}
        {{- $port = index $secret.data "port" | b64dec -}}
      {{- end -}}
      {{- if index $secret.data "username" -}}
        {{- $username = index $secret.data "username" | b64dec -}}
      {{- end -}}
      {{- if index $secret.data "password" -}}
        {{- $password = index $secret.data "password" | b64dec -}}
      {{- end -}}
      {{- if index $secret.data "database" -}}
        {{- $database = index $secret.data "database" | b64dec -}}
      {{- end -}}
    {{- end -}}
  {{- end -}}

  {{- /* Build URI */ -}}
  {{- $uri := "postgresql://" -}}
  {{- if ne $username "" -}}
    {{- $uri = printf "%s%s" $uri $username -}}
    {{- if ne $password "" -}}
      {{- $uri = printf "%s:%s" $uri $password -}}
    {{- end -}}
    {{- $uri = printf "%s@" $uri -}}
  {{- end -}}
  {{- $uri = printf "%s%s" $uri $host -}}
  {{- if ne $port "5432" -}}
    {{- $uri = printf "%s:%s" $uri $port -}}
  {{- end -}}
  {{- $uri = printf "%s/%s" $uri $database -}}
  {{- if .Values.postgres.sslMode -}}
    {{- $uri = printf "%s?sslmode=%s" $uri .Values.postgres.sslMode -}}
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
{{- printf "%s-connection" .Release.Name -}}
{{- end -}}