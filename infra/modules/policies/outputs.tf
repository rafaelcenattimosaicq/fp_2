output "api_endpoint" {
  description = "Policies API Gateway endpoint URL"
  value       = aws_apigatewayv2_stage.default.invoke_url
}

output "policies_bucket_name" {
  description = "S3 bucket name for device policies"
  value       = aws_s3_bucket.policies.id
}
