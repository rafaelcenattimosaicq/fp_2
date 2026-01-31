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

variable "vpc_id" {
  description = "VPC ID"
  type        = string
}

variable "private_subnet_ids" {
  description = "Private subnet IDs for Fargate tasks"
  type        = list(string)
}

variable "mqtt_broker_security_group_id" {
  description = "Security group for the MQTT broker"
  type        = string
}

variable "registry_security_group_id" {
  description = "Security group for the registry"
  type        = string
}

variable "internal_security_group_id" {
  description = "Security group for internal services"
  type        = string
}

variable "ecs_task_execution_role_arn" {
  description = "ECS task execution role ARN"
  type        = string
}

variable "registry_task_role_arn" {
  description = "Registry task role ARN (DynamoDB access)"
  type        = string
}

variable "generic_task_role_arn" {
  description = "Generic task role ARN (CloudWatch only)"
  type        = string
}

variable "secret_arns" {
  description = "Map of secret name to ARN"
  type        = map(string)
}

variable "mqtt_target_group_arn" {
  description = "NLB target group ARN for the MQTT broker"
  type        = string
}

variable "registry_target_group_arn" {
  description = "ALB target group ARN for the registry"
  type        = string
}

variable "nes_coordinator_target_group_arn" {
  description = "ALB target group ARN for the NES coordinator REST API"
  type        = string
}

variable "mqtt_ws_target_group_arn" {
  description = "ALB target group ARN for the MQTT WebSocket endpoint"
  type        = string
}

variable "registry_table_name" {
  description = "DynamoDB table name for the registry"
  type        = string
}

variable "bridge_task_role_arn" {
  description = "Bridge task role ARN (S3 telemetry write)"
  type        = string
  default     = ""
}

variable "telemetry_bucket_name" {
  description = "S3 bucket name for telemetry data (bridge writes here)"
  type        = string
  default     = ""
}

variable "athena_workgroup_name" {
  description = "Athena workgroup name for historical queries"
  type        = string
  default     = ""
}

variable "athena_database_name" {
  description = "Glue database name for Athena queries"
  type        = string
  default     = ""
}

variable "athena_results_bucket_name" {
  description = "S3 bucket name for Athena query results"
  type        = string
  default     = ""
}

variable "nes_nlb_rest_target_group_arn" {
  description = "NLB target group ARN for NES coordinator REST (port 8081)"
  type        = string
  default     = ""
}

variable "nes_nlb_grpc_target_group_arn" {
  description = "NLB target group ARN for NES coordinator gRPC (port 8080)"
  type        = string
  default     = ""
}

variable "nes_nlb_worker_rpc_target_group_arn" {
  description = "NLB target group ARN for NES coordinator worker RPC (port 4000)"
  type        = string
  default     = ""
}

variable "nes_nlb_worker_data_target_group_arn" {
  description = "NLB target group ARN for NES coordinator worker data (port 4001)"
  type        = string
  default     = ""
}

variable "nes_nlb_dns_name" {
  description = "DNS name of the internal NES coordinator NLB"
  type        = string
  default     = ""
}

variable "nes_nlb_zone_id" {
  description = "Route53 zone ID of the internal NES coordinator NLB"
  type        = string
  default     = ""
}

variable "enable_nes_nlb_alias" {
  description = "whether to create a Route53 alias for nes-coordinator pointing to the NLB (must be known at plan time)"
  type        = bool
  default     = false
}
