variable "project" {
  description = "Project name prefix"
  type        = string
}

variable "environment" {
  description = "Deployment environment"
  type        = string
}

variable "alert_email" {
  description = "Email address for alarm notifications"
  type        = string
}

variable "ecs_cluster_name" {
  description = "ECS cluster name for metrics"
  type        = string
}

variable "api_alb_arn_suffix" {
  description = "ALB ARN suffix for CloudWatch metrics"
  type        = string
}

variable "mqtt_nlb_arn_suffix" {
  description = "NLB ARN suffix for CloudWatch metrics"
  type        = string
}

variable "mqtt_target_group_arn_suffix" {
  description = "MQTT target group ARN suffix"
  type        = string
}

variable "registry_target_group_arn_suffix" {
  description = "Registry target group ARN suffix"
  type        = string
}

variable "registry_table_name" {
  description = "DynamoDB table name for throttling alarm"
  type        = string
}
