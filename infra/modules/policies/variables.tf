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

variable "cognito_user_pool_client_id" {
  description = "Cognito App Client ID — used as the JWT audience"
  type        = string
}

variable "gateway_m2m_client_id" {
  description = "Cognito App Client ID for gateway M2M authentication"
  type        = string
}
