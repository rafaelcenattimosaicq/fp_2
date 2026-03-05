locals {
  name_prefix = "${var.project}-${var.environment}"
  has_cert = var.acm_certificate_arn != ""
}

resource "aws_lb" "mqtt" {
  name               = "iot-mqtt-${var.environment}"
  internal           = false
  load_balancer_type = "network"
  subnets            = var.public_subnet_ids

  tags = { Name = "${local.name_prefix}-mqtt-nlb" }
}

resource "aws_lb_target_group" "mqtt" {
  name                 = "${local.name_prefix}-mqtt-tg"
  port                 = 8883
  protocol             = "TCP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 120

  health_check {
    protocol = "TCP"
  }

  tags = { Name = "${local.name_prefix}-mqtt-tg" }
}

resource "aws_lb_listener" "mqtt" {
  load_balancer_arn = aws_lb.mqtt.arn
  port              = 8883
  protocol          = "TCP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.mqtt.arn
  }
}

resource "aws_lb" "api" {
  name               = "iot-api-${var.environment}"
  internal           = false
  load_balancer_type = "application"
  subnets            = var.public_subnet_ids
  security_groups    = [var.alb_security_group_id]

  tags = { Name = "${local.name_prefix}-api-alb" }
}

resource "aws_lb_target_group" "registry" {
  name                 = "${local.name_prefix}-registry-tg"
  port                 = 8088
  protocol             = "HTTP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30

  health_check {
    path                = "/health"
    protocol            = "HTTP"
    interval            = 15
    healthy_threshold   = 2
    unhealthy_threshold = 3
    timeout             = 5
    matcher             = "200"
  }

  tags = { Name = "${local.name_prefix}-registry-tg" }
}

resource "aws_lb_listener" "api_https" {
  count = local.has_cert ? 1 : 0

  load_balancer_arn = aws_lb.api.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = var.acm_certificate_arn

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.registry.arn
  }
}

resource "aws_lb_listener" "api_http_redirect" {
  count = local.has_cert ? 1 : 0

  load_balancer_arn = aws_lb.api.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type = "redirect"

    redirect {
      port        = "443"
      protocol    = "HTTPS"
      status_code = "HTTP_301"
    }
  }
}

resource "aws_lb_listener" "api_http_forward" {
  count = local.has_cert ? 0 : 1

  load_balancer_arn = aws_lb.api.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.registry.arn
  }
}

resource "aws_lb_target_group" "nes_coordinator" {
  name                 = "${local.name_prefix}-nes-tg"
  port                 = 8081
  protocol             = "HTTP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30

  health_check {
    path                = "/v1/nes/connectivity/check"
    protocol            = "HTTP"
    interval            = 15
    healthy_threshold   = 2
    unhealthy_threshold = 3
    timeout             = 5
    matcher             = "200"
  }

  tags = { Name = "${local.name_prefix}-nes-coordinator-tg" }
}

resource "aws_lb_listener_rule" "nes_coordinator_https" {
  count = local.has_cert ? 1 : 0

  listener_arn = aws_lb_listener.api_https[0].arn
  priority     = 100

  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.nes_coordinator.arn
  }

  condition {
    path_pattern {
      values = ["/v1/nes/*"]
    }
  }
}

resource "aws_lb_listener_rule" "nes_coordinator_http" {
  count = local.has_cert ? 0 : 1

  listener_arn = aws_lb_listener.api_http_forward[0].arn
  priority     = 100

  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.nes_coordinator.arn
  }

  condition {
    path_pattern {
      values = ["/v1/nes/*"]
    }
  }
}

resource "aws_lb_target_group" "mqtt_ws" {
  name                 = "${local.name_prefix}-mqtt-ws-tg"
  port                 = 9001
  protocol             = "HTTP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30

  health_check {
    port                = "8080"
    path                = "/"
    protocol            = "HTTP"
    interval            = 30
    healthy_threshold   = 2
    unhealthy_threshold = 3
    timeout             = 5
    matcher             = "200"
  }

  tags = { Name = "${local.name_prefix}-mqtt-ws-tg" }
}

resource "aws_lb_listener_rule" "mqtt_ws_https" {
  count = local.has_cert ? 1 : 0

  listener_arn = aws_lb_listener.api_https[0].arn
  priority     = 90

  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.mqtt_ws.arn
  }

  condition {
    path_pattern {
      values = ["/mqtt"]
    }
  }
}

resource "aws_lb_listener_rule" "mqtt_ws_http" {
  count = local.has_cert ? 0 : 1

  listener_arn = aws_lb_listener.api_http_forward[0].arn
  priority     = 90

  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.mqtt_ws.arn
  }

  condition {
    path_pattern {
      values = ["/mqtt"]
    }
  }
}

resource "aws_lb" "nes_coordinator" {
  name               = "iot-nes-${var.environment}"
  internal           = true
  load_balancer_type = "network"
  subnets            = var.private_subnet_ids
  security_groups    = [var.internal_security_group_id]

  enable_cross_zone_load_balancing = true

  tags = { Name = "${local.name_prefix}-nes-nlb" }
}

resource "aws_lb_target_group" "nes_rest" {
  name                 = "${local.name_prefix}-nes-rest-tg"
  port                 = 8081
  protocol             = "TCP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30

  health_check {
    protocol            = "HTTP"
    path                = "/v1/nes/connectivity/check"
    port                = "8081"
    healthy_threshold   = 2
    unhealthy_threshold = 3
    interval            = 15
  }

  tags = { Name = "${local.name_prefix}-nes-rest-tg" }
}

resource "aws_lb_listener" "nes_rest" {
  load_balancer_arn = aws_lb.nes_coordinator.arn
  port              = 8081
  protocol          = "TCP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.nes_rest.arn
  }
}

resource "aws_lb_target_group" "nes_grpc" {
  name                 = "${local.name_prefix}-nes-grpc-tg"
  port                 = 8080
  protocol             = "TCP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30

  health_check {
    protocol = "TCP"
  }

  tags = { Name = "${local.name_prefix}-nes-grpc-tg" }
}

resource "aws_lb_listener" "nes_grpc" {
  load_balancer_arn = aws_lb.nes_coordinator.arn
  port              = 8080
  protocol          = "TCP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.nes_grpc.arn
  }
}

resource "aws_lb_target_group" "nes_worker_rpc" {
  name                 = "${local.name_prefix}-nes-rpc-tg"
  port                 = 4000
  protocol             = "TCP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30

  health_check {
    protocol = "TCP"
  }

  tags = { Name = "${local.name_prefix}-nes-worker-rpc-tg" }
}

resource "aws_lb_listener" "nes_worker_rpc" {
  load_balancer_arn = aws_lb.nes_coordinator.arn
  port              = 4000
  protocol          = "TCP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.nes_worker_rpc.arn
  }
}

resource "aws_lb_target_group" "nes_worker_data" {
  name                 = "${local.name_prefix}-nes-data-tg"
  port                 = 4001
  protocol             = "TCP"
  vpc_id               = var.vpc_id
  target_type          = "ip"
  deregistration_delay = 30

  health_check {
    protocol = "TCP"
  }

  tags = { Name = "${local.name_prefix}-nes-worker-data-tg" }
}

resource "aws_lb_listener" "nes_worker_data" {
  load_balancer_arn = aws_lb.nes_coordinator.arn
  port              = 4001
  protocol          = "TCP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.nes_worker_data.arn
  }
}

