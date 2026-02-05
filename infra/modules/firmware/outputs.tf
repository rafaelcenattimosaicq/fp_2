output "api_endpoint" {
  description = "Firmware API Gateway endpoint URL"
  value       = aws_apigatewayv2_stage.default.invoke_url
}

output "firmware_bucket_name" {
  description = "S3 bucket name for firmware files"
  value       = aws_s3_bucket.firmware.id
}
