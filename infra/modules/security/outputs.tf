output "ecs_task_execution_role_arn" {
  description = "ARN of the ECS task execution role"
  value       = aws_iam_role.ecs_task_execution.arn
}

output "registry_task_role_arn" {
  description = "ARN of the registry task role (DynamoDB access)"
  value       = aws_iam_role.registry_task.arn
}

output "generic_task_role_arn" {
  description = "ARN of the generic task role (CloudWatch only)"
  value       = aws_iam_role.generic_task.arn
}

output "bridge_task_role_arn" {
  description = "ARN of the bridge task role (S3 telemetry write)"
  value       = aws_iam_role.bridge_task.arn
}

output "secret_arns" {
  description = "Map of secret name to ARN for ECS container secrets"
  value = {
    mtls_ca_cert         = aws_secretsmanager_secret.mtls_ca_cert.arn
    mtls_server_cert     = aws_secretsmanager_secret.mtls_server_cert.arn
    mtls_server_key      = aws_secretsmanager_secret.mtls_server_key.arn
    mtls_client_cert     = aws_secretsmanager_secret.mtls_client_cert.arn
    mtls_client_key      = aws_secretsmanager_secret.mtls_client_key.arn
    registry_admin_token = aws_secretsmanager_secret.registry_admin_token.arn
  }
}

output "acm_certificate_arn" {
  description = "ACM certificate ARN for ALB HTTPS (empty if no domain)"
  value       = length(aws_acm_certificate.alb) > 0 ? aws_acm_certificate.alb[0].arn : ""
}
