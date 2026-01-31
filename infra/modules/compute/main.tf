locals {
  name_prefix = "${var.project}-${var.environment}"

  services = {
    mqtt-broker = {
      cpu            = 256
      memory         = 512
      desired_count  = 1
      container_port = 8883
      use_spot       = false
      security_groups = [
        var.mqtt_broker_security_group_id,
        var.internal_security_group_id
      ]
      task_role_arn    = var.generic_task_role_arn
      target_group_arn = var.mqtt_target_group_arn
      discovery_name   = "mqtt-broker"
    }
    registry = {
      cpu            = 256
      memory         = 512
      desired_count  = 1
      container_port = 8088
      use_spot       = false
      security_groups = [
        var.registry_security_group_id,
        var.internal_security_group_id
      ]
      task_role_arn    = var.registry_task_role_arn
      target_group_arn = var.registry_target_group_arn
      discovery_name   = "registry"
    }
    rules-engine = {
      cpu            = 256
      memory         = 512
      desired_count  = 1
      container_port = 0
      use_spot       = true
      security_groups = [
        var.internal_security_group_id
      ]
      task_role_arn    = var.generic_task_role_arn
      target_group_arn = null
      discovery_name   = "rules-engine"
    }
    mqtt-to-tcp-bridge = {
      cpu            = 256
      memory         = 512
      desired_count  = 1
      container_port = 0
      use_spot       = true
      security_groups = [
        var.internal_security_group_id
      ]
      task_role_arn    = var.bridge_task_role_arn != "" ? var.bridge_task_role_arn : var.generic_task_role_arn
      target_group_arn = null
      discovery_name   = "bridge"
    }
    nes-coordinator = {
      cpu            = 1024
      memory         = 2048
      desired_count  = 1
      container_port = 8081
      use_spot       = false
      security_groups = [
        var.registry_security_group_id,
        var.internal_security_group_id
      ]
      task_role_arn    = var.generic_task_role_arn
      target_group_arn = var.nes_coordinator_target_group_arn
      discovery_name   = "nes-coordinator"
    }
    nebulastream = {
      cpu            = 512
      memory         = 1024
      desired_count  = 1
      container_port = 0
      use_spot       = false
      security_groups = [
        var.internal_security_group_id
      ]
      task_role_arn    = var.generic_task_role_arn
      target_group_arn = null
      discovery_name   = "nebulastream"
    }
  }

  task_def_arns = {
    "mqtt-broker"        = aws_ecs_task_definition.mqtt_broker.arn
    "registry"           = aws_ecs_task_definition.registry.arn
    "rules-engine"       = aws_ecs_task_definition.rules_engine.arn
    "mqtt-to-tcp-bridge" = aws_ecs_task_definition.mqtt_to_tcp_bridge.arn
    "nes-coordinator"    = aws_ecs_task_definition.nes_coordinator.arn
    "nebulastream"       = aws_ecs_task_definition.nebulastream.arn
  }
}

resource "aws_ecr_repository" "services" {
  for_each = local.services

  name = "${var.project}/${each.key}"

  image_scanning_configuration {
    scan_on_push = true
  }

  force_delete = var.environment != "prod"

  tags = {
    Name    = "${local.name_prefix}-${each.key}"
    Service = each.key
  }
}

resource "aws_ecs_cluster" "main" {
  name = local.name_prefix

  setting {
    name  = "containerInsights"
    value = "enabled"
  }

  tags = {
    Name = local.name_prefix
  }
}

resource "aws_ecs_cluster_capacity_providers" "main" {
  cluster_name = aws_ecs_cluster.main.name

  capacity_providers = ["FARGATE", "FARGATE_SPOT"]

  default_capacity_provider_strategy {
    capacity_provider = "FARGATE"
    weight            = 1
    base              = 0
  }
}

resource "aws_service_discovery_private_dns_namespace" "main" {
  name = "iot.local"
  vpc  = var.vpc_id

  tags = {
    Name = "${local.name_prefix}-service-discovery"
  }
}

resource "aws_service_discovery_service" "services" {
  for_each = local.services

  name = each.value.discovery_name

  dns_config {
    namespace_id = aws_service_discovery_private_dns_namespace.main.id

    dns_records {
      type = "A"
      ttl  = 10
    }

    routing_policy = "MULTIVALUE"
  }

  health_check_custom_config {
    failure_threshold = 1
  }

  tags = {
    Name    = "${local.name_prefix}-${each.key}-discovery"
    Service = each.key
  }
}

resource "aws_cloudwatch_log_group" "services" {
  for_each = local.services

  name              = "/ecs/${local.name_prefix}/${each.key}"
  retention_in_days = 30

  tags = {
    Name    = "${local.name_prefix}-${each.key}-logs"
    Service = each.key
  }
}

resource "aws_ecs_task_definition" "mqtt_broker" {
  family                   = "${local.name_prefix}-mqtt-broker"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = local.services["mqtt-broker"].cpu
  memory                   = local.services["mqtt-broker"].memory
  execution_role_arn       = var.ecs_task_execution_role_arn
  task_role_arn            = var.generic_task_role_arn

  container_definitions = jsonencode([
    {
      name      = "mqtt-broker"
      image     = "${aws_ecr_repository.services["mqtt-broker"].repository_url}:latest"
      essential = true
      user      = "10001"

      portMappings = [
        { containerPort = 8883, protocol = "tcp" },
        { containerPort = 1883, protocol = "tcp" },
        { containerPort = 9001, protocol = "tcp" },
        { containerPort = 8080, protocol = "tcp" }
      ]

      environment = [
        { name = "MQTT_BIND_ADDR", value = "0.0.0.0" },
        { name = "MQTT_PORT", value = "8883" },
        { name = "MQTT_ENABLE_PLAINTEXT", value = "true" },
        { name = "MQTT_ENABLE_WS", value = "true" },
        { name = "MQTT_WS_PORT", value = "9001" },
        { name = "MQTT_TLS_CERT_PATH", value = "/run/secrets/server-cert" },
        { name = "MQTT_TLS_KEY_PATH", value = "/run/secrets/server-key" },
        { name = "MQTT_TLS_CA_PATH", value = "/run/secrets/ca-cert" }
      ]

      secrets = [
        { name = "CA_CERT", valueFrom = var.secret_arns["mtls_ca_cert"] },
        { name = "SERVER_CERT", valueFrom = var.secret_arns["mtls_server_cert"] },
        { name = "SERVER_KEY", valueFrom = var.secret_arns["mtls_server_key"] }
      ]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.services["mqtt-broker"].name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  tags = {
    Name    = "${local.name_prefix}-mqtt-broker-task"
    Service = "mqtt-broker"
  }
}

resource "aws_ecs_task_definition" "registry" {
  family                   = "${local.name_prefix}-registry"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = local.services["registry"].cpu
  memory                   = local.services["registry"].memory
  execution_role_arn       = var.ecs_task_execution_role_arn
  task_role_arn            = var.registry_task_role_arn

  container_definitions = jsonencode([
    {
      name      = "registry"
      image     = "${aws_ecr_repository.services["registry"].repository_url}:latest"
      essential = true
      user      = "10001"

      portMappings = [
        { containerPort = 8088, protocol = "tcp" }
      ]

      environment = concat([
        { name = "REGISTRY_BIND_ADDR", value = "0.0.0.0" },
        { name = "REGISTRY_PORT", value = "8088" },
        { name = "REGISTRY_BACKEND", value = "dynamodb" },
        { name = "REGISTRY_DYNAMODB_TABLE", value = var.registry_table_name },
        { name = "AWS_REGION", value = var.aws_region },
        { name = "REGISTRY_MQTT_ENABLED", value = "true" },
        { name = "MQTT_HOST", value = "mqtt-broker.iot.local" },
        { name = "MQTT_PORT", value = "8883" }
        ],
        var.athena_workgroup_name != "" ? [
          { name = "ATHENA_WORKGROUP", value = var.athena_workgroup_name },
          { name = "ATHENA_DATABASE", value = var.athena_database_name },
          { name = "ATHENA_TABLE", value = "compressor_events" },
          { name = "ATHENA_RESULTS_BUCKET", value = var.athena_results_bucket_name }
      ] : [])

      secrets = [
        { name = "ADMIN_TOKEN", valueFrom = var.secret_arns["registry_admin_token"] },
        { name = "CA_CERT", valueFrom = var.secret_arns["mtls_ca_cert"] },
        { name = "CLIENT_CERT", valueFrom = var.secret_arns["mtls_client_cert"] },
        { name = "CLIENT_KEY", valueFrom = var.secret_arns["mtls_client_key"] }
      ]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.services["registry"].name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  tags = {
    Name    = "${local.name_prefix}-registry-task"
    Service = "registry"
  }
}

resource "aws_ecs_task_definition" "rules_engine" {
  family                   = "${local.name_prefix}-rules-engine"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = local.services["rules-engine"].cpu
  memory                   = local.services["rules-engine"].memory
  execution_role_arn       = var.ecs_task_execution_role_arn
  task_role_arn            = var.generic_task_role_arn

  container_definitions = jsonencode([
    {
      name      = "rules-engine"
      image     = "${aws_ecr_repository.services["rules-engine"].repository_url}:latest"
      essential = true
      user      = "10001"

      environment = [
        { name = "MQTT_HOST", value = "mqtt-broker.iot.local" },
        { name = "MQTT_PORT", value = "8883" },
        { name = "MQTT_CLIENT_ID", value = "rules-engine" },
        { name = "POLICY_TOPIC_FILTER", value = "iot/+/policy/#" },
        { name = "TELEMETRY_TOPIC_FILTER", value = "iot/+/telemetry/#" }
      ]

      secrets = [
        { name = "CA_CERT", valueFrom = var.secret_arns["mtls_ca_cert"] },
        { name = "CLIENT_CERT", valueFrom = var.secret_arns["mtls_client_cert"] },
        { name = "CLIENT_KEY", valueFrom = var.secret_arns["mtls_client_key"] }
      ]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.services["rules-engine"].name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  tags = {
    Name    = "${local.name_prefix}-rules-engine-task"
    Service = "rules-engine"
  }
}

resource "aws_ecs_task_definition" "mqtt_to_tcp_bridge" {
  family                   = "${local.name_prefix}-mqtt-to-tcp-bridge"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = local.services["mqtt-to-tcp-bridge"].cpu
  memory                   = local.services["mqtt-to-tcp-bridge"].memory
  execution_role_arn       = var.ecs_task_execution_role_arn
  task_role_arn            = var.bridge_task_role_arn != "" ? var.bridge_task_role_arn : var.generic_task_role_arn

  container_definitions = jsonencode([
    {
      name      = "mqtt-to-tcp-bridge"
      image     = "${aws_ecr_repository.services["mqtt-to-tcp-bridge"].repository_url}:latest"
      essential = true
      user      = "10001"

      environment = concat([
        { name = "MQTT_HOST", value = "mqtt-broker.iot.local" },
        { name = "MQTT_PORT", value = "1883" },
        { name = "MQTT_TOPIC_FILTER", value = "iot/+/telemetry/#" },
        { name = "TCP_BIND_ADDR", value = "nebulastream.iot.local" },
        { name = "TCP_PORT", value = "50501" }
        ],
        var.telemetry_bucket_name != "" ? [
          { name = "S3_BUCKET_NAME", value = var.telemetry_bucket_name },
          { name = "S3_KEY_PREFIX", value = "telemetry" },
          { name = "S3_FLUSH_INTERVAL_SECS", value = "300" },
          { name = "S3_FLUSH_MAX_RECORDS", value = "1000" },
          { name = "AWS_REGION", value = var.aws_region }
      ] : [])

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.services["mqtt-to-tcp-bridge"].name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  tags = {
    Name    = "${local.name_prefix}-mqtt-to-tcp-bridge-task"
    Service = "mqtt-to-tcp-bridge"
  }
}

resource "aws_ecs_task_definition" "nes_coordinator" {
  family                   = "${local.name_prefix}-nes-coordinator"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = local.services["nes-coordinator"].cpu
  memory                   = local.services["nes-coordinator"].memory
  execution_role_arn       = var.ecs_task_execution_role_arn
  task_role_arn            = var.generic_task_role_arn

  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }

  container_definitions = jsonencode([
    {
      name      = "nes-coordinator"
      image     = "451633548946.dkr.ecr.us-east-1.amazonaws.com/iot-platform/nes-coordinator:latest"
      essential = true

      entryPoint = ["/bin/sh", "-c"]
      command = [
        "LOCAL_IP=$(hostname -i | awk '{print $1}') && cat > /tmp/coordinator.yaml << EOF\nlogLevel: LOG_DEBUG\ncoordinatorHost: $LOCAL_IP\nrestIp: 0.0.0.0\nrestPort: 8081\nrpcPort: 8080\nworker:\n  localWorkerHost: $LOCAL_IP\n  rpcPort: 4000\n  dataPort: 4001\nlogicalSources:\n  - logicalSourceName: telemetry\n    fields:\n      - name: DEVICE_ID\n        type: UINT64\n      - name: GATEWAY_ID\n        type: UINT64\n      - name: timestamp\n        type: UINT64\noptimizer:\n  queryMergerRule: DefaultQueryMergerRule\nhealthCheckWaitTime: 5\nEOF\nnesCoordinator --configPath=/tmp/coordinator.yaml"
      ]

      portMappings = [
        { containerPort = 8081, protocol = "tcp" },
        { containerPort = 8080, protocol = "tcp" },
        { containerPort = 4000, protocol = "tcp" },
        { containerPort = 4001, protocol = "tcp" }
      ]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.services["nes-coordinator"].name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  tags = {
    Name    = "${local.name_prefix}-nes-coordinator-task"
    Service = "nes-coordinator"
  }
}

resource "aws_ecs_task_definition" "nebulastream" {
  family                   = "${local.name_prefix}-nebulastream"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = local.services["nebulastream"].cpu
  memory                   = local.services["nebulastream"].memory
  execution_role_arn       = var.ecs_task_execution_role_arn
  task_role_arn            = var.generic_task_role_arn

  container_definitions = jsonencode([
    {
      name      = "nebulastream"
      image     = "nebulastream/nes-executable-image:latest"
      essential = true

      entryPoint = ["/bin/sh", "-c"]
      command = [
        "LOCAL_IP=$(hostname -i | awk '{print $1}') && cat > /tmp/worker.yaml << EOF\nlogLevel: LOG_DEBUG\ncoordinatorHost: nes-coordinator.iot.local\nlocalWorkerHost: $LOCAL_IP\ncoordinatorPort: 8080\nnumberOfSlots: 65535\nphysicalSources:\n  - logicalSourceName: default_logical\n    physicalSourceName: default_physical\n    type: DEFAULT_SOURCE\n    configuration:\n      numberOfBuffersToProduce: 10\n      sourceGatheringInterval: 1000\nEOF\nnesWorker --configPath=/tmp/worker.yaml"
      ]

      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.services["nebulastream"].name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  tags = {
    Name    = "${local.name_prefix}-nebulastream-task"
    Service = "nebulastream"
  }
}

resource "aws_ecs_service" "services" {
  for_each = local.services

  name            = "${local.name_prefix}-${each.key}"
  cluster         = aws_ecs_cluster.main.id
  task_definition = local.task_def_arns[each.key]
  desired_count   = each.value.desired_count

  dynamic "capacity_provider_strategy" {
    for_each = each.value.use_spot ? [
      { provider = "FARGATE_SPOT", weight = 1, base = 1 },
      { provider = "FARGATE", weight = 0, base = 0 }
      ] : [
      { provider = "FARGATE", weight = 1, base = 1 }
    ]
    content {
      capacity_provider = capacity_provider_strategy.value.provider
      weight            = capacity_provider_strategy.value.weight
      base              = capacity_provider_strategy.value.base
    }
  }

  network_configuration {
    subnets          = var.private_subnet_ids
    security_groups  = each.value.security_groups
    assign_public_ip = false
  }

  service_registries {
    registry_arn = aws_service_discovery_service.services[each.key].arn
  }

  deployment_minimum_healthy_percent = 100
  deployment_maximum_percent         = 200

  enable_execute_command = var.environment != "prod"

  dynamic "load_balancer" {
    for_each = each.value.target_group_arn != null ? [each.value.target_group_arn] : []
    content {
      target_group_arn = load_balancer.value
      container_name   = each.key
      container_port   = each.value.container_port
    }
  }

  dynamic "load_balancer" {
    for_each = each.key == "mqtt-broker" ? [var.mqtt_ws_target_group_arn] : []
    content {
      target_group_arn = load_balancer.value
      container_name   = each.key
      container_port   = 9001
    }
  }

  dynamic "load_balancer" {
    for_each = each.key == "nes-coordinator" && var.nes_nlb_rest_target_group_arn != "" ? [1] : []
    content {
      target_group_arn = var.nes_nlb_rest_target_group_arn
      container_name   = "nes-coordinator"
      container_port   = 8081
    }
  }

  dynamic "load_balancer" {
    for_each = each.key == "nes-coordinator" && var.nes_nlb_grpc_target_group_arn != "" ? [1] : []
    content {
      target_group_arn = var.nes_nlb_grpc_target_group_arn
      container_name   = "nes-coordinator"
      container_port   = 8080
    }
  }

  dynamic "load_balancer" {
    for_each = each.key == "nes-coordinator" && var.nes_nlb_worker_rpc_target_group_arn != "" ? [1] : []
    content {
      target_group_arn = var.nes_nlb_worker_rpc_target_group_arn
      container_name   = "nes-coordinator"
      container_port   = 4000
    }
  }

  dynamic "load_balancer" {
    for_each = each.key == "nes-coordinator" && var.nes_nlb_worker_data_target_group_arn != "" ? [1] : []
    content {
      target_group_arn = var.nes_nlb_worker_data_target_group_arn
      container_name   = "nes-coordinator"
      container_port   = 4001
    }
  }

  depends_on = [aws_ecs_cluster_capacity_providers.main]

  tags = {
    Name    = "${local.name_prefix}-${each.key}-service"
    Service = each.key
  }
}

resource "aws_appautoscaling_target" "registry" {
  max_capacity       = 6
  min_capacity       = 1
  resource_id        = "service/${aws_ecs_cluster.main.name}/${aws_ecs_service.services["registry"].name}"
  scalable_dimension = "ecs:service:DesiredCount"
  service_namespace  = "ecs"
}

resource "aws_appautoscaling_policy" "registry_cpu" {
  name               = "${local.name_prefix}-registry-cpu-scaling"
  policy_type        = "TargetTrackingScaling"
  resource_id        = aws_appautoscaling_target.registry.resource_id
  scalable_dimension = aws_appautoscaling_target.registry.scalable_dimension
  service_namespace  = aws_appautoscaling_target.registry.service_namespace

  target_tracking_scaling_policy_configuration {
    predefined_metric_specification {
      predefined_metric_type = "ECSServiceAverageCPUUtilization"
    }

    target_value       = 70.0
    scale_in_cooldown  = 300
    scale_out_cooldown = 60
  }
}
