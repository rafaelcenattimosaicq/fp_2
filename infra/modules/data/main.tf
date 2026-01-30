locals {
  name_prefix = "${var.project}-${var.environment}"
  table_name  = "${local.name_prefix}-registry"
}

resource "aws_dynamodb_table" "registry" {
  name         = local.table_name
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "pk"
  range_key    = "sk"

  attribute {
    name = "pk"
    type = "S"
  }

  attribute {
    name = "sk"
    type = "S"
  }

  attribute {
    name = "entity_type"
    type = "S"
  }

  attribute {
    name = "status"
    type = "S"
  }

  global_secondary_index {
    name            = "gsi-entity-status"
    hash_key        = "entity_type"
    range_key       = "status"
    projection_type = "ALL"
  }

  global_secondary_index {
    name            = "gsi-gateway-devices"
    hash_key        = "pk"
    range_key       = "entity_type"
    projection_type = "ALL"
  }

  ttl {
    attribute_name = "expires_at"
    enabled        = true
  }

  point_in_time_recovery {
    enabled = true
  }

  deletion_protection_enabled = var.environment == "prod"

  server_side_encryption {
    enabled = true
  }

  tags = { Name = local.table_name }
}
