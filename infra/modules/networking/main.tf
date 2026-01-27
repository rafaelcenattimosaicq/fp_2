locals {
  name_prefix = "${var.project}-${var.environment}"
  azs = ["${var.aws_region}a", "${var.aws_region}b"]
}

resource "aws_vpc" "main" {
  cidr_block           = var.vpc_cidr
  enable_dns_support   = true
  enable_dns_hostnames = true

  tags = {
    Name = "${local.name_prefix}-vpc"
  }
}

resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id

  tags = {
    Name = "${local.name_prefix}-igw"
  }
}

resource "aws_subnet" "public" {
  count = 2

  vpc_id                  = aws_vpc.main.id
  cidr_block              = cidrsubnet(var.vpc_cidr, 8, count.index + 1)
  availability_zone       = local.azs[count.index]
  map_public_ip_on_launch = true

  tags = {
    Name = "${local.name_prefix}-public-${local.azs[count.index]}"
  }
}

resource "aws_subnet" "private" {
  count = 2

  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 8, count.index + 10)
  availability_zone = local.azs[count.index]

  tags = {
    Name = "${local.name_prefix}-private-${local.azs[count.index]}"
  }
}

data "aws_ami" "amazon_linux_2023" {
  most_recent = true
  owners      = ["amazon"]

  filter {
    name   = "name"
    values = ["al2023-ami-*-arm64"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }

  filter {
    name   = "architecture"
    values = ["arm64"]
  }
}

resource "aws_security_group" "nat_instance" {
  name_prefix = "${local.name_prefix}-nat-"
  description = "NAT instance - allow traffic from private subnets"
  vpc_id      = aws_vpc.main.id

  tags = {
    Name = "${local.name_prefix}-sg-nat"
  }
}

resource "aws_vpc_security_group_ingress_rule" "nat_from_private" {
  count = 2

  security_group_id = aws_security_group.nat_instance.id
  description       = "All traffic from private subnet ${count.index}"
  cidr_ipv4         = aws_subnet.private[count.index].cidr_block
  ip_protocol       = "-1"
}

resource "aws_vpc_security_group_egress_rule" "nat_all" {
  security_group_id = aws_security_group.nat_instance.id
  description       = "Allow all outbound"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}

data "aws_iam_instance_profile" "nat" {
  count = var.nat_instance_profile_name != "" ? 1 : 0
  name  = var.nat_instance_profile_name
}

resource "aws_iam_role_policy" "nat_self_register" {
  count = var.nat_instance_profile_name != "" ? 1 : 0

  name = "${local.name_prefix}-nat-self-register"
  role = data.aws_iam_instance_profile.nat[0].role_name

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid      = "DisableSourceDestCheck"
        Effect   = "Allow"
        Action   = ["ec2:ModifyInstanceAttribute"]
        Resource = "arn:aws:ec2:${var.aws_region}:*:instance/*"
      },
      {
        Sid    = "ManagePrivateRoute"
        Effect = "Allow"
        Action = [
          "ec2:ReplaceRoute",
          "ec2:CreateRoute",
          "ec2:DescribeRouteTables"
        ]
        Resource = "*"
      }
    ]
  })
}

resource "aws_launch_template" "nat" {
  name_prefix   = "${local.name_prefix}-nat-"
  image_id      = data.aws_ami.amazon_linux_2023.id
  instance_type = var.nat_instance_type

  network_interfaces {
    associate_public_ip_address = true
    security_groups             = [aws_security_group.nat_instance.id]
  }

  dynamic "iam_instance_profile" {
    for_each = var.nat_instance_profile_name != "" ? [var.nat_instance_profile_name] : []
    content {
      name = iam_instance_profile.value
    }
  }

  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 2
  }

  user_data = base64encode(templatefile("${path.module}/nat_user_data.sh.tftpl", {
    vpc_cidr              = var.vpc_cidr
    aws_region            = var.aws_region
    route_table_id        = aws_route_table.private.id
    tailscale_secret_name = var.tailscale_secret_name
    project               = var.project
    environment           = var.environment
  }))

  tag_specifications {
    resource_type = "instance"
    tags = {
      Name = "${local.name_prefix}-nat-instance"
    }
  }

  tags = {
    Name = "${local.name_prefix}-nat-lt"
  }
}

resource "aws_autoscaling_group" "nat" {
  name_prefix         = "${local.name_prefix}-nat-"
  min_size            = 1
  max_size            = 1
  desired_capacity    = 1
  vpc_zone_identifier = [aws_subnet.public[0].id]

  launch_template {
    id      = aws_launch_template.nat.id
    version = "$Latest"
  }

  health_check_type         = "EC2"
  health_check_grace_period = 120

  instance_refresh {
    strategy = "Rolling"
    preferences {
      min_healthy_percentage = 0
    }
  }

  tag {
    key                 = "Name"
    value               = "${local.name_prefix}-nat-instance"
    propagate_at_launch = true
  }

  tag {
    key                 = "ManagedBy"
    value               = "terraform"
    propagate_at_launch = true
  }

  tag {
    key                 = "Project"
    value               = var.project
    propagate_at_launch = true
  }

  tag {
    key                 = "Environment"
    value               = var.environment
    propagate_at_launch = true
  }
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id

  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }

  tags = {
    Name = "${local.name_prefix}-public-rt"
  }
}

resource "aws_route_table_association" "public" {
  count = 2

  subnet_id      = aws_subnet.public[count.index].id
  route_table_id = aws_route_table.public.id
}

resource "aws_route_table" "private" {
  vpc_id = aws_vpc.main.id

  tags = {
    Name = "${local.name_prefix}-private-rt"
  }

  lifecycle {
    ignore_changes = [route]
  }
}

resource "aws_route_table_association" "private" {
  count = 2

  subnet_id      = aws_subnet.private[count.index].id
  route_table_id = aws_route_table.private.id
}

resource "aws_vpc_endpoint" "dynamodb" {
  vpc_id            = aws_vpc.main.id
  service_name      = "com.amazonaws.${var.aws_region}.dynamodb"
  vpc_endpoint_type = "Gateway"
  route_table_ids   = [aws_route_table.private.id]

  tags = {
    Name = "${local.name_prefix}-vpce-dynamodb"
  }
}

resource "aws_vpc_endpoint" "s3" {
  vpc_id            = aws_vpc.main.id
  service_name      = "com.amazonaws.${var.aws_region}.s3"
  vpc_endpoint_type = "Gateway"
  route_table_ids   = [aws_route_table.private.id]

  tags = {
    Name = "${local.name_prefix}-vpce-s3"
  }
}

resource "aws_security_group" "alb" {
  name_prefix = "${local.name_prefix}-alb-"
  description = "ALB - allow inbound HTTPS from the internet"
  vpc_id      = aws_vpc.main.id

  tags = {
    Name = "${local.name_prefix}-sg-alb"
  }
}

resource "aws_vpc_security_group_ingress_rule" "alb_https" {
  security_group_id = aws_security_group.alb.id
  description       = "HTTPS from the internet"
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 443
  to_port           = 443
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "alb_http" {
  security_group_id = aws_security_group.alb.id
  description       = "HTTP from the internet (dev only)"
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 80
  to_port           = 80
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_egress_rule" "alb_all" {
  security_group_id = aws_security_group.alb.id
  description       = "Allow all outbound"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}

resource "aws_security_group" "mqtt_broker" {
  name_prefix = "${local.name_prefix}-mqtt-broker-"
  description = "MQTT broker - allow inbound MQTTS (8883) from the internet via NLB"
  vpc_id      = aws_vpc.main.id

  tags = {
    Name = "${local.name_prefix}-sg-mqtt-broker"
  }
}

resource "aws_vpc_security_group_ingress_rule" "mqtt_broker_mqtts" {
  security_group_id = aws_security_group.mqtt_broker.id
  description       = "MQTTS from the internet (NLB passthrough)"
  cidr_ipv4         = "0.0.0.0/0"
  from_port         = 8883
  to_port           = 8883
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "mqtt_broker_ws_from_alb" {
  security_group_id            = aws_security_group.mqtt_broker.id
  description                  = "MQTT WebSocket from ALB (Cloud Desktop browser clients)"
  referenced_security_group_id = aws_security_group.alb.id
  from_port                    = 9001
  to_port                      = 9001
  ip_protocol                  = "tcp"
}

resource "aws_vpc_security_group_egress_rule" "mqtt_broker_all" {
  security_group_id = aws_security_group.mqtt_broker.id
  description       = "Allow all outbound"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}

resource "aws_security_group" "registry" {
  name_prefix = "${local.name_prefix}-registry-"
  description = "Registry - allow inbound from ALB on port 8088"
  vpc_id      = aws_vpc.main.id

  tags = {
    Name = "${local.name_prefix}-sg-registry"
  }
}

resource "aws_vpc_security_group_ingress_rule" "registry_from_alb" {
  security_group_id            = aws_security_group.registry.id
  description                  = "HTTP from ALB on registry port"
  referenced_security_group_id = aws_security_group.alb.id
  from_port                    = 8088
  to_port                      = 8088
  ip_protocol                  = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "nes_coordinator_from_alb" {
  security_group_id            = aws_security_group.registry.id
  description                  = "HTTP from ALB on NES coordinator port"
  referenced_security_group_id = aws_security_group.alb.id
  from_port                    = 8081
  to_port                      = 8081
  ip_protocol                  = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "nes_rest_from_vpc" {
  security_group_id = aws_security_group.internal.id
  description       = "NES REST from VPC via NLB"
  cidr_ipv4         = aws_vpc.main.cidr_block
  from_port         = 8081
  to_port           = 8081
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "nes_rest_from_tailscale" {
  security_group_id = aws_security_group.internal.id
  description       = "NES REST from Tailscale edge workers"
  cidr_ipv4         = "100.64.0.0/10"
  from_port         = 8081
  to_port           = 8081
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "nes_grpc_from_tailscale" {
  security_group_id = aws_security_group.registry.id
  description       = "NES gRPC from edge workers via Tailscale VPN"
  cidr_ipv4         = "100.64.0.0/10"
  from_port         = 8080
  to_port           = 8080
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "nes_grpc_from_vpc" {
  security_group_id = aws_security_group.registry.id
  description       = "NES gRPC from workers in VPC"
  cidr_ipv4         = aws_vpc.main.cidr_block
  from_port         = 8080
  to_port           = 8080
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "nes_data_from_tailscale" {
  security_group_id = aws_security_group.registry.id
  description       = "NES coordinator internal worker gRPC from Tailscale VPN"
  cidr_ipv4         = "100.64.0.0/10"
  from_port         = 4000
  to_port           = 4001
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "nes_data_from_vpc" {
  security_group_id = aws_security_group.registry.id
  description       = "NES coordinator internal worker ports from VPC"
  cidr_ipv4         = aws_vpc.main.cidr_block
  from_port         = 4000
  to_port           = 4001
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_egress_rule" "registry_all" {
  security_group_id = aws_security_group.registry.id
  description       = "Allow all outbound"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}

resource "aws_security_group" "internal" {
  name_prefix = "${local.name_prefix}-internal-"
  description = "Internal service-to-service communication"
  vpc_id      = aws_vpc.main.id

  tags = {
    Name = "${local.name_prefix}-sg-internal"
  }
}

resource "aws_vpc_security_group_ingress_rule" "internal_self" {
  security_group_id            = aws_security_group.internal.id
  description                  = "All TCP from members of this security group"
  referenced_security_group_id = aws_security_group.internal.id
  from_port                    = 0
  to_port                      = 65535
  ip_protocol                  = "tcp"
}

resource "aws_vpc_security_group_egress_rule" "internal_all" {
  security_group_id = aws_security_group.internal.id
  description       = "Allow all outbound"
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "-1"
}

resource "aws_vpc_security_group_ingress_rule" "mqtt_from_internal" {
  security_group_id            = aws_security_group.mqtt_broker.id
  description                  = "MQTT from internal services"
  referenced_security_group_id = aws_security_group.internal.id
  from_port                    = 1883
  to_port                      = 1883
  ip_protocol                  = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "mqtt_from_tailscale" {
  security_group_id = aws_security_group.mqtt_broker.id
  description       = "MQTT from edge workers via Tailscale"
  cidr_ipv4         = "100.64.0.0/10"
  from_port         = 1883
  to_port           = 1883
  ip_protocol       = "tcp"
}

resource "aws_vpc_security_group_ingress_rule" "mqtt_mtls_from_internal" {
  security_group_id            = aws_security_group.mqtt_broker.id
  description                  = "MQTTS from internal services"
  referenced_security_group_id = aws_security_group.internal.id
  from_port                    = 8883
  to_port                      = 8883
  ip_protocol                  = "tcp"
}

resource "aws_cloudwatch_log_group" "flow_logs" {
  name              = "/vpc/${local.name_prefix}/flow-logs"
  retention_in_days = 14

  tags = {
    Name = "${local.name_prefix}-vpc-flow-logs"
  }
}

resource "aws_iam_role" "flow_logs" {
  name_prefix = "${local.name_prefix}-flow-logs-"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Principal = {
          Service = "vpc-flow-logs.amazonaws.com"
        }
        Action = "sts:AssumeRole"
      }
    ]
  })

  tags = {
    Name = "${local.name_prefix}-flow-logs-role"
  }
}

resource "aws_iam_role_policy" "flow_logs" {
  name_prefix = "${local.name_prefix}-flow-logs-"
  role        = aws_iam_role.flow_logs.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "logs:CreateLogGroup",
          "logs:CreateLogStream",
          "logs:PutLogEvents",
          "logs:DescribeLogGroups",
          "logs:DescribeLogStreams"
        ]
        Resource = "${aws_cloudwatch_log_group.flow_logs.arn}:*"
      }
    ]
  })
}

resource "aws_flow_log" "main" {
  vpc_id               = aws_vpc.main.id
  traffic_type         = "ALL"
  log_destination_type = "cloud-watch-logs"
  log_destination      = aws_cloudwatch_log_group.flow_logs.arn
  iam_role_arn         = aws_iam_role.flow_logs.arn

  tags = {
    Name = "${local.name_prefix}-vpc-flow-log"
  }
}
