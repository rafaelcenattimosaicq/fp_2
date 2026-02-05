output "api_url" {
  description = "VPN Provisioner API Gateway endpoint URL"
  value       = aws_apigatewayv2_stage.default.invoke_url
}

output "api_id" {
  description = "VPN Provisioner API Gateway ID"
  value       = aws_apigatewayv2_api.vpn.id
}
