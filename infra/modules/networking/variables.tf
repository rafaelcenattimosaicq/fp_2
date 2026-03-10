variable "project" {
  description = "Project name prefix"
  type        = string
}

variable "environment" {
  description = "Deployment environment"
  type        = string
}

variable "vpc_cidr" {
  description = "CIDR block for the VPC"
  type        = string
}

variable "aws_region" {
  description = "AWS region for resource placement"
  type        = string
}

variable "nat_instance_type" {
  description = "EC2 instance type for the NAT instance (cost-optimised default: t4g.nano ~$3/mo)"
  type        = string
  default     = "t4g.nano"
}

variable "tailscale_secret_name" {
  description = "Secrets Manager secret name holding the Tailscale API key (used by NAT instance to create auth keys at boot)"
  type        = string
  default     = ""
}

variable "nat_instance_profile_name" {
  description = "IAM instance profile name for the NAT instance (SSM + Secrets Manager access)"
  type        = string
  default     = ""
}
