variable "project" {
  description = "Project name prefix"
  type        = string
}

variable "environment" {
  description = "Deployment environment"
  type        = string
}

variable "aws_region" {
  description = "AWS region"
  type        = string
}

variable "cognito_user_pool_arn" {
  description = "ARN of the Cognito User Pool for JWT authorizer"
  type        = string
}

variable "cognito_user_pool_client_ids" {
  description = "Cognito App Client IDs — used as the JWT audience (supports both user and M2M clients)"
  type        = list(string)
}

variable "tailscale_api_key_secret_arn" {
  description = "ARN of the Secrets Manager secret holding the Tailscale API key"
  type        = string
}

variable "tailscale_tailnet" {
  description = "Tailscale tailnet name (org). Use '-' for the default tailnet."
  type        = string
  default     = "-"
}

variable "coordinator_host" {
  description = "Hostname of the NES coordinator service (resolved via Cloud Map)"
  type        = string
  default     = "nes-coordinator.iot.local"
}

variable "coordinator_grpc_port" {
  description = "gRPC port for the NES coordinator"
  type        = number
  default     = 8080
}

variable "coordinator_rest_port" {
  description = "REST port for the NES coordinator"
  type        = number
  default     = 8081
}

variable "cloudmap_namespace" {
  description = "Cloud Map namespace for dynamic service IP resolution"
  type        = string
  default     = "iot.local"
}

variable "mqtt_broker_service" {
  description = "Cloud Map service name for the MQTT broker"
  type        = string
  default     = "mqtt-broker"
}

variable "coordinator_service" {
  description = "Cloud Map service name for the NES coordinator (manually-managed)"
  type        = string
  default     = "nes-coordinator"
}
