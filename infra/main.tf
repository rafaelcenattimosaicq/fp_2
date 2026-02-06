provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = var.project
      Environment = var.environment
      ManagedBy   = "terraform"
    }
  }
}

data "aws_caller_identity" "current" {}

module "networking" {
  source = "./modules/networking"

  project     = var.project
  environment = var.environment
  vpc_cidr    = var.vpc_cidr
  aws_region  = var.aws_region

  tailscale_secret_name     = "iot/${var.environment}/tailscale-api-key"
  nat_instance_profile_name = "iot-platform-staging-nat-instance"
}

module "telemetry" {
  source = "./modules/telemetry"

  project     = var.project
  environment = var.environment
  aws_region  = var.aws_region
}

module "security" {
  source = "./modules/security"

  project              = var.project
  environment          = var.environment
  aws_region           = var.aws_region
  registry_admin_token = var.registry_admin_token
  domain_name          = var.domain_name

  telemetry_bucket_arn      = module.telemetry.telemetry_bucket_arn
  athena_results_bucket_arn = module.telemetry.athena_results_bucket_arn
  glue_database_name        = module.telemetry.glue_database_name
}

module "data" {
  source = "./modules/data"

  project     = var.project
  environment = var.environment
}

module "auth" {
  source = "./modules/auth"

  project     = var.project
  environment = var.environment
}

module "loadbalancing" {
  source = "./modules/loadbalancing"

  project                    = var.project
  environment                = var.environment
  vpc_id                     = module.networking.vpc_id
  public_subnet_ids          = module.networking.public_subnet_ids
  private_subnet_ids         = module.networking.private_subnet_ids
  alb_security_group_id      = module.networking.alb_security_group_id
  internal_security_group_id = module.networking.internal_security_group_id
  acm_certificate_arn        = module.security.acm_certificate_arn
}

module "compute" {
  source = "./modules/compute"

  project     = var.project
  environment = var.environment
  aws_region  = var.aws_region

  vpc_id                        = module.networking.vpc_id
  private_subnet_ids            = module.networking.private_subnet_ids
  mqtt_broker_security_group_id = module.networking.mqtt_broker_security_group_id
  registry_security_group_id    = module.networking.registry_security_group_id
  internal_security_group_id    = module.networking.internal_security_group_id

  ecs_task_execution_role_arn = module.security.ecs_task_execution_role_arn
  registry_task_role_arn      = module.security.registry_task_role_arn
  generic_task_role_arn       = module.security.generic_task_role_arn
  bridge_task_role_arn        = module.security.bridge_task_role_arn
  secret_arns                 = module.security.secret_arns

  mqtt_target_group_arn            = module.loadbalancing.mqtt_target_group_arn
  mqtt_ws_target_group_arn         = module.loadbalancing.mqtt_ws_target_group_arn
  registry_target_group_arn        = module.loadbalancing.registry_target_group_arn
  nes_coordinator_target_group_arn = module.loadbalancing.nes_coordinator_target_group_arn
  nes_nlb_rest_target_group_arn        = module.loadbalancing.nes_nlb_rest_target_group_arn
  nes_nlb_grpc_target_group_arn        = module.loadbalancing.nes_nlb_grpc_target_group_arn
  nes_nlb_worker_rpc_target_group_arn  = module.loadbalancing.nes_nlb_worker_rpc_target_group_arn
  nes_nlb_worker_data_target_group_arn = module.loadbalancing.nes_nlb_worker_data_target_group_arn
  nes_nlb_dns_name     = module.loadbalancing.nes_nlb_dns
  nes_nlb_zone_id      = module.loadbalancing.nes_nlb_zone_id
  enable_nes_nlb_alias = true
  registry_table_name              = module.data.registry_table_name

  telemetry_bucket_name      = module.telemetry.telemetry_bucket_name
  athena_workgroup_name      = module.telemetry.athena_workgroup_name
  athena_database_name       = module.telemetry.glue_database_name
  athena_results_bucket_name = module.telemetry.athena_results_bucket_name
}

module "monitoring" {
  source = "./modules/monitoring"

  project     = var.project
  environment = var.environment
  alert_email = var.alert_email

  ecs_cluster_name                 = module.compute.ecs_cluster_name
  api_alb_arn_suffix               = module.loadbalancing.api_alb_arn_suffix
  mqtt_nlb_arn_suffix              = module.loadbalancing.mqtt_nlb_arn_suffix
  mqtt_target_group_arn_suffix     = module.loadbalancing.mqtt_target_group_arn_suffix
  registry_target_group_arn_suffix = module.loadbalancing.registry_target_group_arn_suffix
  registry_table_name              = module.data.registry_table_name
}

module "policies" {
  source = "./modules/policies"

  project                     = var.project
  environment                 = var.environment
  aws_region                  = var.aws_region
  cognito_user_pool_arn       = module.auth.user_pool_arn
  cognito_user_pool_client_id = module.auth.user_pool_client_id
  gateway_m2m_client_id       = module.auth.gateway_m2m_client_id
}

module "firmware" {
  source = "./modules/firmware"

  project                     = var.project
  environment                 = var.environment
  aws_region                  = var.aws_region
  cognito_user_pool_arn       = module.auth.user_pool_arn
  cognito_user_pool_client_id = module.auth.user_pool_client_id
}

module "devices" {
  source = "./modules/devices"

  project                     = var.project
  environment                 = var.environment
  aws_region                  = var.aws_region
  cognito_user_pool_arn       = module.auth.user_pool_arn
  cognito_user_pool_client_id = module.auth.user_pool_client_id
}

module "vpn" {
  source = "./modules/vpn"

  project                      = var.project
  environment                  = var.environment
  aws_region                   = var.aws_region
  cognito_user_pool_arn        = module.auth.user_pool_arn
  cognito_user_pool_client_ids = [
    module.auth.user_pool_client_id,
    module.auth.gateway_m2m_client_id,
  ]
  tailscale_api_key_secret_arn = "arn:aws:secretsmanager:${var.aws_region}:${data.aws_caller_identity.current.account_id}:secret:iot/${var.environment}/tailscale-api-key-*"
}
