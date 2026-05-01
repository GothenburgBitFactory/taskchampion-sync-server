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
  {{- $secret := .Values.postgres.existingSecret -}}
  {{- $secretData := "" -}}
  {{- if $secret -}}
    {{- $secretData = (lookup "v1" "Secret" .Release.Namespace $secret).data -}}
  {{- end -}}
  
  {{- $host := "" -}}
  {{- $port := "5432" -}}
  {{- $username := "" -}}
  {{- $password := "" -}}
  {{- $database := "taskchampion" -}}
  
  {{- /* Get values from secret (higher priority) */ -}}
  {{- if $secretData -}}
    {{- if hasKey $secretData "host" -}}
      {{- $host = (b64dec $secretData.host) -}}
    {{- end -}}
    {{- if hasKey $secretData "port" -}}
      {{- $port = (b64dec $secretData.port) -}}
    {{- end -}}
    {{- if hasKey $secretData "username" -}}
      {{- $username = (b64dec $secretData.username) -}}
    {{- end -}}
    {{- if hasKey $secretData "password" -}}
      {{- $password = (b64dec $secretData.password) -}}
    {{- end -}}
    {{- if hasKey $secretData "database" -}}
      {{- $database = (b64dec $secretData.database) -}}
    {{- end -}}
  {{- end -}}
  
  {{- /* Fallback to values.yaml */ -}}
  {{- if eq $host "" -}}
    {{- $host = .Values.postgres.host -}}
  {{- end -}}
  {{- if eq $username "" -}}
    {{- $username = .Values.postgres.username -}}
  {{- end -}}
  {{- if eq $password "" -}}
    {{- $password = .Values.postgres.password -}}
  {{- end -}}
  {{- if eq $database "" -}}
    {{- $database = .Values.postgres.database -}}
  {{- end -}}
  
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
  {{- if ne $port "5432" -}}
    {{- $uri = printf "%s:%s" $uri $port -}}
  {{- end -}}
  {{- if ne $database "taskchampion" -}}
    {{- $uri = printf "%s/%s" $uri $database -}}
  {{- end -}}
  {{- $uri -}}
{{- end -}}
