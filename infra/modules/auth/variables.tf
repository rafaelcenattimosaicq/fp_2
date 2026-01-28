variable "project" {
  description = "Project name prefix"
  type        = string
}

variable "environment" {
  description = "Deployment environment"
  type        = string
}
  # TODO: make callback URLs configurable per environment
  # The Cognito hosted UI needs explicit callback URLs for each
  # deployment target (localhost dev, staging ALB, prod CloudFront).
  # variable "cognito_callback_urls" {
  #   type    = list(string)
  #   default = ["http://localhost:5173/callback"]
  # }
