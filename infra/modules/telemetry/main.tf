locals {
  name_prefix = "${var.project}-${var.environment}"
}

resource "aws_s3_bucket" "telemetry" {
  bucket = "${local.name_prefix}-telemetry"

  force_destroy = var.environment != "prod"

  tags = {
    Name = "${local.name_prefix}-telemetry"
  }
}

resource "aws_s3_bucket_versioning" "telemetry" {
  bucket = aws_s3_bucket.telemetry.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "telemetry" {
  bucket = aws_s3_bucket.telemetry.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_public_access_block" "telemetry" {
  bucket = aws_s3_bucket.telemetry.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "telemetry" {
  bucket = aws_s3_bucket.telemetry.id

  rule {
    id     = "archive-and-expire"
    status = "Enabled"

    filter {
      prefix = "telemetry/"
    }

    transition {
      days          = 90
      storage_class = "GLACIER"
    }

    expiration {
      days = 365
    }
  }
}

resource "aws_s3_bucket" "athena_results" {
  bucket = "${local.name_prefix}-athena-results"

  force_destroy = var.environment != "prod"

  tags = {
    Name = "${local.name_prefix}-athena-results"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "athena_results" {
  bucket = aws_s3_bucket.athena_results.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_public_access_block" "athena_results" {
  bucket = aws_s3_bucket.athena_results.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_lifecycle_configuration" "athena_results" {
  bucket = aws_s3_bucket.athena_results.id

  rule {
    id     = "expire-results"
    status = "Enabled"

    filter {
      prefix = ""
    }

    expiration {
      days = 7
    }
  }
}

resource "aws_glue_catalog_database" "telemetry" {
  name = "${replace(local.name_prefix, "-", "_")}_telemetry"
}

resource "aws_glue_catalog_table" "compressor_events" {
  name          = "compressor_events"
  database_name = aws_glue_catalog_database.telemetry.name

  table_type = "EXTERNAL_TABLE"

  parameters = {
    "projection.enabled" = "true"

    "projection.year.type"  = "integer"
    "projection.year.range" = "2024,2030"

    "projection.month.type"   = "integer"
    "projection.month.range"  = "01,12"
    "projection.month.digits" = "2"

    "projection.day.type"   = "integer"
    "projection.day.range"  = "01,31"
    "projection.day.digits" = "2"

    "projection.hour.type"   = "integer"
    "projection.hour.range"  = "00,23"
    "projection.hour.digits" = "2"

    "storage.location.template" = "s3://${aws_s3_bucket.telemetry.id}/telemetry/year=$${year}/month=$${month}/day=$${day}/hour=$${hour}/"

    "classification" = "json"
  }

  storage_descriptor {
    location      = "s3://${aws_s3_bucket.telemetry.id}/telemetry/"
    input_format  = "org.apache.hadoop.mapred.TextInputFormat"
    output_format = "org.apache.hadoop.hive.ql.io.HiveIgnoreKeyTextOutputFormat"

    ser_de_info {
      serialization_library = "org.openx.data.jsonserde.JsonSerDe"

      parameters = {
        "case.insensitive" = "true"
      }
    }

    columns {
      name = "device_id"
      type = "string"
    }
    columns {
      name = "temperature"
      type = "double"
    }
    columns {
      name = "voltage"
      type = "double"
    }
    columns {
      name = "current"
      type = "double"
    }
    columns {
      name = "power"
      type = "double"
    }
    columns {
      name = "pressure"
      type = "double"
    }
    columns {
      name = "compressor_speed"
      type = "double"
    }
    columns {
      name = "frequency"
      type = "double"
    }
    columns {
      name = "state_of_charge"
      type = "double"
    }
    columns {
      name = "state"
      type = "string"
    }
    columns {
      name = "ingest_ts"
      type = "string"
    }
  }

  partition_keys {
    name = "year"
    type = "string"
  }
  partition_keys {
    name = "month"
    type = "string"
  }
  partition_keys {
    name = "day"
    type = "string"
  }
  partition_keys {
    name = "hour"
    type = "string"
  }
}

resource "aws_athena_workgroup" "telemetry" {
  name = "${local.name_prefix}-telemetry"

  configuration {
    enforce_workgroup_configuration = true

    result_configuration {
      output_location = "s3://${aws_s3_bucket.athena_results.id}/results/"
    }

    bytes_scanned_cutoff_per_query = 104857600
  }

  tags = {
    Name = "${local.name_prefix}-telemetry-workgroup"
  }
}
