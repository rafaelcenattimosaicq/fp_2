variable "project" {
  description = "Project name prefix"
  type        = string
}

variable "environment" {
  description = "Deployment environment"
  type        = string
}

variable "vpc_id" {
  description = "VPC ID"
  type        = string
}

variable "public_subnet_ids" {
  description = "Public subnet IDs for load balancers"
  type        = list(string)
}

variable "alb_security_group_id" {
  description = "Security group ID for the ALB"
  type        = string
}

variable "acm_certificate_arn" {
  description = "ACM certificate ARN for HTTPS. Empty string uses HTTP listener."
  type        = string
  default     = ""
}

variable "private_subnet_ids" {
  description = "Private subnet IDs for the internal NLB"
  type        = list(string)
}

variable "internal_security_group_id" {
  description = "Security group ID for internal service-to-service traffic"
  type        = string
}
