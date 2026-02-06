output "mqtt_endpoint" {
  description = "MQTT broker endpoint (NLB DNS:8883)"
  value       = "${module.loadbalancing.mqtt_nlb_dns}:8883"
}

output "api_endpoint" {
  description = "Registry API endpoint (ALB DNS)"
  value       = module.loadbalancing.api_alb_dns
}

output "cognito_user_pool_id" {
  description = "Cognito User Pool ID for cloud-desktop config"
  value       = module.auth.user_pool_id
}

output "cognito_client_id" {
  description = "Cognito App Client ID for cloud-desktop config"
  value       = module.auth.user_pool_client_id
}

output "ecr_repository_urls" {
  description = "ECR repository URLs for docker push"
  value       = module.compute.ecr_repository_urls
}

output "ecs_cluster_name" {
  description = "ECS cluster name"
  value       = module.compute.ecs_cluster_name
}

output "dynamodb_table_name" {
  description = "DynamoDB registry table name"
  value       = module.data.registry_table_name
}

output "dashboard_url" {
  description = "CloudWatch dashboard URL"
  value       = "https://console.aws.amazon.com/cloudwatch/home?region=${var.aws_region}#dashboards:name=${module.monitoring.dashboard_name}"
}

output "telemetry_bucket_name" {
  description = "S3 bucket name for historical telemetry data"
  value       = module.telemetry.telemetry_bucket_name
}

output "athena_workgroup" {
  description = "Athena workgroup for historical telemetry queries"
  value       = module.telemetry.athena_workgroup_name
}

output "policies_api_endpoint" {
  description = "Device Policies API Gateway endpoint URL"
  value       = module.policies.api_endpoint
}

output "policies_bucket_name" {
  description = "S3 bucket name for device policies"
  value       = module.policies.policies_bucket_name
}

output "devices_api_endpoint" {
  description = "Device Registry API Gateway endpoint URL"
  value       = module.devices.api_endpoint
}

output "devices_table_name" {
  description = "DynamoDB table name for device records"
  value       = module.devices.devices_table_name
}

output "nes_coordinator_endpoint" {
  description = "NES coordinator REST API endpoint (via ALB at /v1/nes/)"
  value       = "http://${module.loadbalancing.api_alb_dns}/v1/nes"
}

output "vpn_provisioner_url" {
  description = "VPN Provisioner API Gateway endpoint URL"
  value       = module.vpn.api_url
}

output "gateway_m2m_client_id" {
  description = "Cognito M2M client ID for gateway auth"
  value       = module.auth.gateway_m2m_client_id
}

output "gateway_m2m_client_secret" {
  description = "Cognito M2M client secret for gateway auth"
  value       = module.auth.gateway_m2m_client_secret
  sensitive   = true
}

output "cognito_domain" {
  description = "Cognito User Pool domain prefix for OAuth2 token endpoint"
  value       = module.auth.user_pool_domain
}
