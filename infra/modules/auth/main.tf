locals {
  name_prefix = "${var.project}-${var.environment}"
}

resource "aws_cognito_user_pool" "main" {
  name = "${local.name_prefix}-users"

  mfa_configuration = "OPTIONAL"

  software_token_mfa_configuration {
    enabled = true
  }

  password_policy {
    minimum_length                   = 12
    require_uppercase                = true
    require_lowercase                = true
    require_numbers                  = true
    require_symbols                  = true
    temporary_password_validity_days = 7
  }

  account_recovery_setting {
    recovery_mechanism {
      name     = "verified_email"
      priority = 1
    }
  }

  schema {
    name                = "email"
    attribute_data_type = "String"
    required            = true
    mutable             = true

    string_attribute_constraints {
      min_length = 1
      max_length = 256
    }
  }

  schema {
    name                = "role"
    attribute_data_type = "String"
    required            = false
    mutable             = true

    string_attribute_constraints {
      min_length = 1
      max_length = 32
    }
  }

  user_attribute_update_settings {
    attributes_require_verification_before_update = ["email"]
  }

  auto_verified_attributes = ["email"]

  admin_create_user_config {
    allow_admin_create_user_only = false

    invite_message_template {
      email_subject = "Welcome to Cloud Desktop"
      email_message = <<-HTML
        <div style="max-width:480px;margin:0 auto;font-family:'Segoe UI',-apple-system,system-ui,sans-serif;background:#1e1e1e;border-radius:4px;overflow:hidden">
          <div style="background:#217346;padding:14px 24px;display:flex;align-items:center">
            <span style="color:#fff;font-size:15px;font-weight:600;letter-spacing:.02em">Cloud Desktop</span>
          </div>
          <div style="padding:28px 24px">
            <h2 style="margin:0 0 8px;color:#e8e8e8;font-size:18px;font-weight:600">Your account is ready</h2>
            <p style="margin:0 0 20px;color:#808080;font-size:13px">An administrator has created a Cloud Desktop account for you.</p>
            <table style="width:100%;border-collapse:collapse;margin-bottom:20px">
              <tr>
                <td style="padding:10px 12px;background:#252526;border:1px solid #3e3e42;color:#808080;font-size:12px;font-weight:600;text-transform:uppercase;letter-spacing:.04em;width:110px;border-radius:2px 0 0 0">Username</td>
                <td style="padding:10px 12px;background:#2d2d30;border:1px solid #3e3e42;color:#e8e8e8;font-size:13px;font-family:'SF Mono','Cascadia Code','Consolas',monospace;border-radius:0 2px 0 0">{username}</td>
              </tr>
              <tr>
                <td style="padding:10px 12px;background:#252526;border:1px solid #3e3e42;color:#808080;font-size:12px;font-weight:600;text-transform:uppercase;letter-spacing:.04em;border-radius:0 0 0 2px">Temp password</td>
                <td style="padding:10px 12px;background:#2d2d30;border:1px solid #3e3e42;color:#e8e8e8;font-size:13px;font-family:'SF Mono','Cascadia Code','Consolas',monospace;border-radius:0 0 2px 0">{####}</td>
              </tr>
            </table>
            <p style="margin:0 0 8px;color:#cccccc;font-size:12px">You will be asked to set a permanent password and configure MFA on first sign-in.</p>
            <p style="margin:0;color:#808080;font-size:11px">This temporary password expires in 7 days.</p>
          </div>
          <div style="background:#217346;padding:8px 24px">
            <span style="color:rgba(255,255,255,.7);font-size:11px">Cloud Desktop &mdash; IoT Device Management</span>
          </div>
        </div>
      HTML
      sms_message   = "Your Cloud Desktop username is {username} and temporary password is {####}"
    }
  }

  verification_message_template {
    default_email_option = "CONFIRM_WITH_CODE"
    email_subject        = "Cloud Desktop — Verification Code"
    email_message        = <<-HTML
      <div style="max-width:480px;margin:0 auto;font-family:'Segoe UI',-apple-system,system-ui,sans-serif;background:#1e1e1e;border-radius:4px;overflow:hidden">
        <div style="background:#217346;padding:14px 24px">
          <span style="color:#fff;font-size:15px;font-weight:600;letter-spacing:.02em">Cloud Desktop</span>
        </div>
        <div style="padding:28px 24px;text-align:center">
          <h2 style="margin:0 0 8px;color:#e8e8e8;font-size:18px;font-weight:600">Verification Code</h2>
          <p style="margin:0 0 24px;color:#808080;font-size:13px">Use the code below to complete your request.</p>
          <div style="display:inline-block;padding:14px 32px;background:#252526;border:1px solid #3e3e42;border-radius:2px;margin-bottom:24px">
            <span style="color:#33956b;font-size:28px;font-weight:700;font-family:'SF Mono','Cascadia Code','Consolas',monospace;letter-spacing:.35em">{####}</span>
          </div>
          <p style="margin:0;color:#808080;font-size:11px">If you did not request this code, you can safely ignore this email.</p>
        </div>
        <div style="background:#217346;padding:8px 24px">
          <span style="color:rgba(255,255,255,.7);font-size:11px">Cloud Desktop &mdash; IoT Device Management</span>
        </div>
      </div>
    HTML
  }

  tags = { Name = "${local.name_prefix}-user-pool" }
}

resource "aws_cognito_user_pool_client" "cloud_desktop" {
  name         = "cloud-desktop-${var.environment}"
  user_pool_id = aws_cognito_user_pool.main.id

  explicit_auth_flows = [
    "ALLOW_USER_SRP_AUTH",
    "ALLOW_USER_PASSWORD_AUTH",
    "ALLOW_REFRESH_TOKEN_AUTH"
  ]

  access_token_validity  = 1
  id_token_validity      = 1
  refresh_token_validity = 30

  token_validity_units {
    access_token  = "hours"
    id_token      = "hours"
    refresh_token = "days"
  }

  generate_secret = false

  prevent_user_existence_errors = "ENABLED"
}

resource "aws_cognito_user_group" "administrators" {
  name         = "administrators"
  user_pool_id = aws_cognito_user_pool.main.id
  description  = "Full access - device management, user management, policies"
}

resource "aws_cognito_user_group" "maintenance" {
  name         = "maintenance"
  user_pool_id = aws_cognito_user_pool.main.id
  description  = "Device configuration and telemetry access"
}

resource "aws_cognito_user_group" "operators" {
  name         = "operators"
  user_pool_id = aws_cognito_user_pool.main.id
  description  = "Read-only telemetry and device status"
}

resource "aws_cognito_user_pool_domain" "main" {
  domain       = "${local.name_prefix}-auth"
  user_pool_id = aws_cognito_user_pool.main.id
}

resource "aws_cognito_resource_server" "policies_api" {
  identifier   = "policies-api"
  name         = "Policies API"
  user_pool_id = aws_cognito_user_pool.main.id

  scope {
    scope_name        = "read"
    scope_description = "Read-only access to device policies"
  }
}

resource "aws_cognito_resource_server" "vpn_api" {
  identifier   = "vpn-api"
  name         = "VPN Provisioner API"
  user_pool_id = aws_cognito_user_pool.main.id

  scope {
    scope_name        = "provision"
    scope_description = "Provision and manage VPN keys for edge gateways"
  }
}

resource "aws_cognito_user_pool_client" "gateway_m2m" {
  name         = "gateway-m2m-${var.environment}"
  user_pool_id = aws_cognito_user_pool.main.id

  generate_secret              = true
  allowed_oauth_flows          = ["client_credentials"]
  allowed_oauth_scopes         = ["policies-api/read", "vpn-api/provision"]
  allowed_oauth_flows_user_pool_client = true

  supported_identity_providers = ["COGNITO"]

  depends_on = [
    aws_cognito_resource_server.policies_api,
    aws_cognito_resource_server.vpn_api,
  ]
}
