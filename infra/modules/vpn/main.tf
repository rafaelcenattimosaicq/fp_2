locals {
  name_prefix   = "${var.project}-${var.environment}"
  function_name = "${local.name_prefix}-vpn-provisioner"
}

resource "aws_dynamodb_table" "vpn_requests" {
  name         = "${local.name_prefix}-vpn-requests"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "gateway_id"

  attribute {
    name = "gateway_id"
    type = "S"
  }

  ttl {
    attribute_name = "ttl"
    enabled        = true
  }

  tags = { Name = "${local.name_prefix}-vpn-requests" }
}

resource "aws_dynamodb_table" "gateway_registry" {
  name         = "${local.name_prefix}-gateway-registry"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "gateway_id"

  attribute {
    name = "gateway_id"
    type = "S"
  }

  point_in_time_recovery { enabled = true }

  tags = { Name = "${local.name_prefix}-gateway-registry" }
}

resource "aws_iam_role" "lambda" {
  name = "${local.function_name}-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action    = "sts:AssumeRole"
      Effect    = "Allow"
      Principal = { Service = "lambda.amazonaws.com" }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "lambda_basic" {
  role       = aws_iam_role.lambda.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

resource "aws_iam_role_policy" "lambda_secrets" {
  name = "${local.function_name}-secrets-access"
  role = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["secretsmanager:GetSecretValue"]
      Resource = [var.tailscale_api_key_secret_arn]
    }]
  })
}

resource "aws_iam_role_policy" "lambda_dynamodb" {
  name = "${local.function_name}-dynamodb-access"
  role = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = [
        "dynamodb:GetItem",
        "dynamodb:PutItem",
        "dynamodb:UpdateItem",
        "dynamodb:DeleteItem",
        "dynamodb:Scan",
      ]
      Resource = [
        aws_dynamodb_table.vpn_requests.arn,
        aws_dynamodb_table.gateway_registry.arn,
      ]
    }]
  })
}

resource "aws_iam_role_policy" "lambda_cloudmap" {
  name = "${local.function_name}-cloudmap-access"
  role = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["servicediscovery:DiscoverInstances"]
      Resource = ["*"]
    }]
  })
}

resource "aws_iam_role_policy" "lambda_s3_releases" {
  name = "${local.function_name}-s3-releases-access"
  role = aws_iam_role.lambda.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["s3:GetObject", "s3:HeadObject"]
      Resource = ["arn:aws:s3:::iot-platform-gateway-releases/*"]
    }]
  })
}

data "archive_file" "lambda_zip" {
  type        = "zip"
  source_dir  = "${path.module}/lambda"
  output_path = "${path.module}/handler.zip"
}

resource "aws_lambda_function" "handler" {
  function_name    = local.function_name
  role             = aws_iam_role.lambda.arn
  handler          = "handler.lambda_handler"
  runtime          = "python3.12"
  filename         = data.archive_file.lambda_zip.output_path
  source_code_hash = data.archive_file.lambda_zip.output_base64sha256
  timeout          = 15
  memory_size      = 128

  environment {
    variables = {
      TAILSCALE_API_KEY_SECRET_NAME = "iot/${var.environment}/tailscale-api-key"
      TAILSCALE_TAILNET             = var.tailscale_tailnet
      COORDINATOR_HOST              = var.coordinator_host
      COORDINATOR_GRPC_PORT         = tostring(var.coordinator_grpc_port)
      COORDINATOR_REST_PORT         = tostring(var.coordinator_rest_port)
      REQUESTS_TABLE                = aws_dynamodb_table.vpn_requests.name
      REGISTRY_TABLE                = aws_dynamodb_table.gateway_registry.name
      CLOUDMAP_NAMESPACE            = var.cloudmap_namespace
      MQTT_BROKER_SERVICE           = var.mqtt_broker_service
      COORDINATOR_SERVICE           = var.coordinator_service
      RELEASES_BUCKET               = "iot-platform-gateway-releases"
    }
  }
}

resource "aws_apigatewayv2_api" "vpn" {
  name          = "${local.name_prefix}-vpn-api"
  protocol_type = "HTTP"

  cors_configuration {
    allow_origins = ["tauri://localhost", "https://tauri.localhost"]
    allow_methods = ["GET", "POST", "DELETE", "OPTIONS"]
    allow_headers = ["Content-Type", "Authorization"]
    max_age       = 3600
  }
}

resource "aws_apigatewayv2_authorizer" "cognito" {
  api_id           = aws_apigatewayv2_api.vpn.id
  name             = "cognito-jwt"
  authorizer_type  = "JWT"
  identity_sources = ["$request.header.Authorization"]

  jwt_configuration {
    audience = var.cognito_user_pool_client_ids
    issuer   = "https://cognito-idp.${var.aws_region}.amazonaws.com/${split("/", var.cognito_user_pool_arn)[1]}"
  }
}

resource "aws_apigatewayv2_integration" "lambda" {
  api_id                 = aws_apigatewayv2_api.vpn.id
  integration_type       = "AWS_PROXY"
  integration_uri        = aws_lambda_function.handler.invoke_arn
  payload_format_version = "2.0"
}

resource "aws_apigatewayv2_route" "request" {
  api_id    = aws_apigatewayv2_api.vpn.id
  route_key = "POST /vpn/request"
  target    = "integrations/${aws_apigatewayv2_integration.lambda.id}"
}

resource "aws_apigatewayv2_route" "poll" {
  api_id    = aws_apigatewayv2_api.vpn.id
  route_key = "GET /vpn/poll/{request_token}"
  target    = "integrations/${aws_apigatewayv2_integration.lambda.id}"
}

resource "aws_apigatewayv2_route" "provision" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "POST /vpn/provision"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "requests_list" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "GET /vpn/requests"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "approve" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "POST /vpn/approve/{gateway_id}"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "status" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "GET /vpn/status/{gateway_id}"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "revoke" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "DELETE /vpn/revoke/{gateway_id}"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "register_gateway" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "POST /vpn/gateways"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "list_gateways" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "GET /vpn/gateways"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "get_gateway" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "GET /vpn/gateways/{gateway_id}"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "delete_gateway" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "DELETE /vpn/gateways/{gateway_id}"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_route" "download" {
  api_id             = aws_apigatewayv2_api.vpn.id
  route_key          = "GET /vpn/download/{arch}"
  target             = "integrations/${aws_apigatewayv2_integration.lambda.id}"
  authorization_type = "JWT"
  authorizer_id      = aws_apigatewayv2_authorizer.cognito.id
}

resource "aws_apigatewayv2_stage" "default" {
  api_id      = aws_apigatewayv2_api.vpn.id
  name        = "$default"
  auto_deploy = true

  default_route_settings {
    throttling_burst_limit = 10
    throttling_rate_limit  = 5
  }
}

resource "aws_lambda_permission" "apigw" {
  statement_id  = "AllowAPIGateway"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.handler.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.vpn.execution_arn}/*/*"
}
