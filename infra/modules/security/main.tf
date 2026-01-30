locals {
  name_prefix = "${var.project}-${var.environment}"
}

data "aws_iam_policy_document" "ecs_assume_role" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "ecs_task_execution" {
  name               = "${local.name_prefix}-ecs-task-execution"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume_role.json

  tags = {
    Name = "${local.name_prefix}-ecs-task-execution"
  }
}

resource "aws_iam_role_policy_attachment" "ecs_task_execution_managed" {
  role       = aws_iam_role.ecs_task_execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role_policy" "ecs_task_execution_secrets" {
  name = "${local.name_prefix}-execution-secrets"
  role = aws_iam_role.ecs_task_execution.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["secretsmanager:GetSecretValue"]
        Resource = "arn:aws:secretsmanager:${var.aws_region}:*:secret:iot/${var.environment}/*"
      }
    ]
  })
}

resource "aws_iam_role" "registry_task" {
  name               = "${local.name_prefix}-registry-task"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume_role.json

  tags = {
    Name = "${local.name_prefix}-registry-task"
  }
}

resource "aws_iam_role_policy" "registry_dynamodb" {
  name = "${local.name_prefix}-registry-dynamodb"
  role = aws_iam_role.registry_task.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "dynamodb:GetItem",
          "dynamodb:PutItem",
          "dynamodb:UpdateItem",
          "dynamodb:DeleteItem",
          "dynamodb:Query",
          "dynamodb:Scan"
        ]
        Resource = "arn:aws:dynamodb:${var.aws_region}:*:table/${local.name_prefix}-registry*"
      }
    ]
  })
}

resource "aws_iam_role_policy" "registry_athena" {
  count = var.telemetry_bucket_arn != "" ? 1 : 0

  name = "${local.name_prefix}-registry-athena"
  role = aws_iam_role.registry_task.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "AthenaQueryExecution"
        Effect = "Allow"
        Action = [
          "athena:StartQueryExecution",
          "athena:GetQueryExecution",
          "athena:GetQueryResults",
          "athena:StopQueryExecution"
        ]
        Resource = "arn:aws:athena:${var.aws_region}:*:workgroup/${local.name_prefix}-telemetry"
      },
      {
        Sid    = "GlueCatalogRead"
        Effect = "Allow"
        Action = [
          "glue:GetDatabase",
          "glue:GetTable",
          "glue:GetPartitions"
        ]
        Resource = [
          "arn:aws:glue:${var.aws_region}:*:catalog",
          "arn:aws:glue:${var.aws_region}:*:database/${var.glue_database_name}",
          "arn:aws:glue:${var.aws_region}:*:table/${var.glue_database_name}/*"
        ]
      },
      {
        Sid    = "S3ReadTelemetry"
        Effect = "Allow"
        Action = ["s3:GetObject", "s3:ListBucket", "s3:GetBucketLocation"]
        Resource = [var.telemetry_bucket_arn, "${var.telemetry_bucket_arn}/*"]
      },
      {
        Sid    = "S3WriteAthenaResults"
        Effect = "Allow"
        Action = ["s3:PutObject", "s3:GetObject", "s3:ListBucket", "s3:GetBucketLocation"]
        Resource = [var.athena_results_bucket_arn, "${var.athena_results_bucket_arn}/*"]
      }
    ]
  })
}

resource "aws_iam_role" "generic_task" {
  name               = "${local.name_prefix}-generic-task"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume_role.json

  tags = {
    Name = "${local.name_prefix}-generic-task"
  }
}

resource "aws_iam_role" "bridge_task" {
  name               = "${local.name_prefix}-bridge-task"
  assume_role_policy = data.aws_iam_policy_document.ecs_assume_role.json

  tags = {
    Name = "${local.name_prefix}-bridge-task"
  }
}

resource "aws_secretsmanager_secret" "mtls_ca_cert" {
  name = "iot/${var.environment}/mtls/ca-cert-${local.name_prefix}"

  tags = {
    Name = "${local.name_prefix}-mtls-ca-cert"
  }
}

resource "aws_secretsmanager_secret" "mtls_server_cert" {
  name = "iot/${var.environment}/mtls/server-cert-${local.name_prefix}"

  tags = {
    Name = "${local.name_prefix}-mtls-server-cert"
  }
}

resource "aws_secretsmanager_secret" "mtls_server_key" {
  name = "iot/${var.environment}/mtls/server-key-${local.name_prefix}"

  tags = {
    Name = "${local.name_prefix}-mtls-server-key"
  }
}

resource "aws_secretsmanager_secret" "mtls_client_cert" {
  name = "iot/${var.environment}/mtls/client-cert-${local.name_prefix}"

  tags = {
    Name = "${local.name_prefix}-mtls-client-cert"
  }
}

resource "aws_secretsmanager_secret" "mtls_client_key" {
  name = "iot/${var.environment}/mtls/client-key-${local.name_prefix}"

  tags = {
    Name = "${local.name_prefix}-mtls-client-key"
  }
}

resource "aws_secretsmanager_secret" "registry_admin_token" {
  name = "iot/${var.environment}/registry/admin-token-${local.name_prefix}"
  tags = { Name = "${local.name_prefix}-registry-admin-token" }
}

resource "aws_secretsmanager_secret_version" "registry_admin_token" {
  secret_id     = aws_secretsmanager_secret.registry_admin_token.id
  secret_string = var.registry_admin_token
}

resource "aws_acm_certificate" "alb" {
  count             = var.domain_name != "" ? 1 : 0
  domain_name       = "*.${var.domain_name}"
  validation_method = "DNS"

  lifecycle {
    create_before_destroy = true
  }

  tags = { Name = "${local.name_prefix}-alb-cert" }
}
