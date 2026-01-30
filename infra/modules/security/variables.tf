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

variable "registry_admin_token" {
  description = "Admin bearer token for the registry API"
  type        = string
  sensitive   = true
}

variable "domain_name" {
  description = "Domain name for ACM certificate. Empty string skips ACM."
  type        = string
  default     = ""
}

variable "telemetry_bucket_arn" {
  description = "ARN of the S3 telemetry bucket (for bridge write and registry read)"
  type        = string
  default     = ""
}

variable "athena_results_bucket_arn" {
  description = "ARN of the S3 Athena results bucket (for registry write)"
  type        = string
  default     = ""
}

variable "glue_database_name" {
  description = "Glue catalog database name (for registry Athena access)"
  type        = string
  default     = ""
}
  # TODO: add WAF integration for ALB
  # variable "enable_waf" {
  #   description = "Enable AWS WAF on the public ALB"
  #   type        = bool
  #   default     = false
  # }
