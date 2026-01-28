output "user_pool_id" {
  description = "Cognito User Pool ID"
  value       = aws_cognito_user_pool.main.id
}

output "user_pool_client_id" {
  description = "Cognito App Client ID for cloud-desktop"
  value       = aws_cognito_user_pool_client.cloud_desktop.id
}

output "user_pool_arn" {
  description = "Cognito User Pool ARN"
  value       = aws_cognito_user_pool.main.arn
}

output "gateway_m2m_client_id" {
  description = "Cognito App Client ID for gateway M2M auth"
  value       = aws_cognito_user_pool_client.gateway_m2m.id
}

output "user_pool_domain" {
  description = "Cognito User Pool domain prefix"
  value       = aws_cognito_user_pool_domain.main.domain
}

output "gateway_m2m_client_secret" {
  description = "Cognito App Client secret for gateway M2M auth"
  value       = aws_cognito_user_pool_client.gateway_m2m.client_secret
  sensitive   = true
}
