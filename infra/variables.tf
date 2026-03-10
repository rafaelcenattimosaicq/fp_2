variable "project" {
  description = "Project name prefix for all resources"
  type        = string
  default     = "iot-platform"
}

variable "environment" {
  description = "Deployment environment (dev, staging, prod)"
  type        = string

  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be dev, staging, or prod."
  }
}

variable "aws_region" {
  description = "AWS region for all resources"
  type        = string
  default     = "us-east-1"
}

variable "vpc_cidr" {
  description = "CIDR block for the VPC"
  type        = string
  default     = "10.0.0.0/16"
}

variable "alert_email" {
  description = "Email address for CloudWatch alarm notifications"
  type        = string
}

variable "domain_name" {
  description = "Domain name for ACM certificate (e.g. iot.example.com). Leave empty to skip ACM."
  type        = string
  default     = ""
}

variable "registry_admin_token" {
  description = "Admin bearer token for the registry API"
  type        = string
  sensitive   = true
}
