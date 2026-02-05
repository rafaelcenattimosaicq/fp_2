variable "project" {
  description = "Project name prefix (shared across all infra modules)"
  type        = string
}

variable "environment" {
  description = "Deployment environment"
  type        = string
}
