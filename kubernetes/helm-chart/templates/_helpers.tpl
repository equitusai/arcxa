{{- define "imagePullSecret" }}
{{- with .Values.imageCredentials }}
{{- printf "{\"auths\":{\"%s\":{\"auth\":\"%s\"}}}" .registry (printf "%s:%s" .username .password | b64enc) | b64enc }}
{{- end }}
{{- end }}

{{/*
Create a comma-separated string of Kafka broker addresses
*/}}
{{- define "kafka.brokerList" -}}
{{- $brokers := list -}}
{{- $name := "graphica-kafka" -}}
{{- $serviceName := printf "%s-brokers" $name -}}
{{- $port := 9092 -}}
{{- range $i, $e := until (int .Values.kafka.replicas) -}}
{{- $brokers = append $brokers (printf "%s-%d.%s:%d" $name $i $serviceName $port) -}}
{{- end -}}
{{- join "," $brokers -}}
{{- end -}}