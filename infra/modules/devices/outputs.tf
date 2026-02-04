output "api_endpoint" {
  description = "Devices API Gateway endpoint URL"
  value       = aws_apigatewayv2_stage.default.invoke_url
}

output "devices_table_name" {
  description = "DynamoDB table name for device records"
  value       = aws_dynamodb_table.devices.name
}
