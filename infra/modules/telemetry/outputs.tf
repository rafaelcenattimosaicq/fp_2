output "telemetry_bucket_name" {
  description = "S3 bucket for telemetry data (partitioned by year/month/day/hour)"
  value       = aws_s3_bucket.telemetry.id
}

output "telemetry_bucket_arn" {
  description = "S3 bucket ARN for telemetry data (used in IAM policies)"
  value       = aws_s3_bucket.telemetry.arn
}

output "athena_results_bucket_name" {
  description = "S3 bucket name for Athena query results"
  value       = aws_s3_bucket.athena_results.id
}

output "athena_results_bucket_arn" {
  description = "S3 bucket ARN for Athena query results (used in IAM policies)"
  value       = aws_s3_bucket.athena_results.arn
}

output "athena_workgroup_name" {
  description = "Athena workgroup name for query execution"
  value       = aws_athena_workgroup.telemetry.name
}

output "glue_database_name" {
  description = "Glue catalog database name for Athena queries"
  value       = aws_glue_catalog_database.telemetry.name
}
