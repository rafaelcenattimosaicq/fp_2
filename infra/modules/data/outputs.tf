output "registry_table_name" {
  description = "DynamoDB registry table name (single-table design)"
  value       = aws_dynamodb_table.registry.name
}
