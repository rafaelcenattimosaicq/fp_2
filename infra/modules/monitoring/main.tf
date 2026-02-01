locals {
  name_prefix = "${var.project}-${var.environment}"

  service_keys = [
    "mqtt-broker",
    "registry",
    "rules-engine",
    "mqtt-to-tcp-bridge",
    "nebulastream",
  ]
}

data "aws_caller_identity" "current" {}
data "aws_region" "current" {}

resource "aws_sns_topic" "alerts" {
  name = "${local.name_prefix}-alerts"

  tags = {
    Name = "${local.name_prefix}-alerts"
  }
}

resource "aws_sns_topic_subscription" "email" {
  topic_arn = aws_sns_topic.alerts.arn
  protocol  = "email"
  endpoint  = var.alert_email
}

resource "aws_cloudwatch_metric_alarm" "mqtt_broker_unhealthy" {
  alarm_name          = "${local.name_prefix}-mqtt-broker-unhealthy"
  alarm_description   = "MQTT broker NLB has unhealthy targets"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 2
  metric_name         = "UnHealthyHostCount"
  namespace           = "AWS/NetworkELB"
  period              = 60
  statistic           = "Maximum"
  threshold           = 0

  dimensions = {
    TargetGroup  = var.mqtt_target_group_arn_suffix
    LoadBalancer = var.mqtt_nlb_arn_suffix
  }

  alarm_actions = [aws_sns_topic.alerts.arn]
  ok_actions    = [aws_sns_topic.alerts.arn]

  tags = {
    Name = "${local.name_prefix}-mqtt-broker-unhealthy"
  }
}

resource "aws_cloudwatch_metric_alarm" "registry_high_cpu" {
  alarm_name          = "${local.name_prefix}-registry-high-cpu"
  alarm_description   = "Registry ECS service CPU > 80% for 5 min"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 5
  metric_name         = "CPUUtilization"
  namespace           = "AWS/ECS"
  period              = 60
  statistic           = "Average"
  threshold           = 80

  dimensions = {
    ClusterName = var.ecs_cluster_name
    ServiceName = "${local.name_prefix}-registry"
  }

  alarm_actions = [aws_sns_topic.alerts.arn]
  ok_actions    = [aws_sns_topic.alerts.arn]

  tags = {
    Name = "${local.name_prefix}-registry-high-cpu"
  }
}

resource "aws_cloudwatch_metric_alarm" "registry_5xx_errors" {
  alarm_name          = "${local.name_prefix}-registry-5xx-errors"
  alarm_description   = "Registry ALB 5xx errors > 10 in 5 min"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 1
  metric_name         = "HTTPCode_Target_5XX_Count"
  namespace           = "AWS/ApplicationELB"
  period              = 300
  statistic           = "Sum"
  threshold           = 10
  treat_missing_data  = "notBreaching"

  dimensions = {
    LoadBalancer = var.api_alb_arn_suffix
  }

  alarm_actions = [aws_sns_topic.alerts.arn]

  tags = {
    Name = "${local.name_prefix}-registry-5xx-errors"
  }
}

resource "aws_cloudwatch_metric_alarm" "dynamodb_throttled" {
  alarm_name          = "${local.name_prefix}-dynamodb-throttled"
  alarm_description   = "DynamoDB throttled requests detected"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = 1
  metric_name         = "ThrottledRequests"
  namespace           = "AWS/DynamoDB"
  period              = 60
  statistic           = "Sum"
  threshold           = 0
  treat_missing_data  = "notBreaching"

  dimensions = {
    TableName = var.registry_table_name
  }

  alarm_actions = [aws_sns_topic.alerts.arn]

  tags = {
    Name = "${local.name_prefix}-dynamodb-throttled"
  }
}

resource "aws_cloudwatch_event_rule" "ecs_task_failure" {
  name        = "${local.name_prefix}-ecs-task-failure"
  description = "Detect ECS tasks stopping with non-zero exit code"

  event_pattern = jsonencode({
    source      = ["aws.ecs"]
    detail-type = ["ECS Task State Change"]
    detail = {
      clusterArn = ["arn:aws:ecs:${data.aws_region.current.name}:${data.aws_caller_identity.current.account_id}:cluster/${var.ecs_cluster_name}"]
      lastStatus = ["STOPPED"]
      containers = {
        exitCode = [{ "anything-but" : 0 }]
      }
    }
  })

  tags = {
    Name = "${local.name_prefix}-ecs-task-failure"
  }
}

resource "aws_cloudwatch_event_target" "ecs_failure_to_sns" {
  rule      = aws_cloudwatch_event_rule.ecs_task_failure.name
  target_id = "send-to-sns"
  arn       = aws_sns_topic.alerts.arn
}

resource "aws_sns_topic_policy" "allow_eventbridge" {
  arn = aws_sns_topic.alerts.arn

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid       = "AllowEventBridgePublish"
        Effect    = "Allow"
        Principal = { Service = "events.amazonaws.com" }
        Action    = "sns:Publish"
        Resource  = aws_sns_topic.alerts.arn
      }
    ]
  })
}

resource "aws_cloudwatch_dashboard" "main" {
  dashboard_name = local.name_prefix

  dashboard_body = jsonencode({
    widgets = concat(
      [
        {
          type   = "metric"
          x      = 0
          y      = 0
          width  = 24
          height = 6
          properties = {
            title  = "ECS CPU Utilization"
            region = data.aws_region.current.name
            metrics = [
              for svc in local.service_keys : [
                "AWS/ECS", "CPUUtilization",
                "ClusterName", var.ecs_cluster_name,
                "ServiceName", "${local.name_prefix}-${svc}",
                { label = svc }
              ]
            ]
            period = 300
            stat   = "Average"
            view   = "timeSeries"
          }
        }
      ],

      [
        {
          type   = "metric"
          x      = 0
          y      = 6
          width  = 24
          height = 6
          properties = {
            title  = "ECS Memory Utilization"
            region = data.aws_region.current.name
            metrics = [
              for svc in local.service_keys : [
                "AWS/ECS", "MemoryUtilization",
                "ClusterName", var.ecs_cluster_name,
                "ServiceName", "${local.name_prefix}-${svc}",
                { label = svc }
              ]
            ]
            period = 300
            stat   = "Average"
            view   = "timeSeries"
          }
        }
      ],

      [
        {
          type   = "metric"
          x      = 0
          y      = 12
          width  = 24
          height = 6
          properties = {
            title  = "NLB Active Connections (MQTT)"
            region = data.aws_region.current.name
            metrics = [
              [
                "AWS/NetworkELB", "ActiveFlowCount",
                "LoadBalancer", var.mqtt_nlb_arn_suffix
              ]
            ]
            period = 300
            stat   = "Average"
            view   = "timeSeries"
          }
        }
      ],

      [
        {
          type   = "metric"
          x      = 0
          y      = 18
          width  = 12
          height = 6
          properties = {
            title  = "ALB Request Count (Registry)"
            region = data.aws_region.current.name
            metrics = [
              [
                "AWS/ApplicationELB", "RequestCount",
                "LoadBalancer", var.api_alb_arn_suffix
              ]
            ]
            period = 300
            stat   = "Sum"
            view   = "timeSeries"
          }
        },
        {
          type   = "metric"
          x      = 12
          y      = 18
          width  = 12
          height = 6
          properties = {
            title  = "ALB Target Response Time (Registry)"
            region = data.aws_region.current.name
            metrics = [
              [
                "AWS/ApplicationELB", "TargetResponseTime",
                "LoadBalancer", var.api_alb_arn_suffix
              ]
            ]
            period = 300
            stat   = "Average"
            view   = "timeSeries"
          }
        }
      ],

      [
        {
          type   = "metric"
          x      = 0
          y      = 24
          width  = 8
          height = 6
          properties = {
            title  = "DynamoDB Read Capacity"
            region = data.aws_region.current.name
            metrics = [
              [
                "AWS/DynamoDB", "ConsumedReadCapacityUnits",
                "TableName", var.registry_table_name
              ]
            ]
            period = 300
            stat   = "Sum"
            view   = "timeSeries"
          }
        },
        {
          type   = "metric"
          x      = 8
          y      = 24
          width  = 8
          height = 6
          properties = {
            title  = "DynamoDB Write Capacity"
            region = data.aws_region.current.name
            metrics = [
              [
                "AWS/DynamoDB", "ConsumedWriteCapacityUnits",
                "TableName", var.registry_table_name
              ]
            ]
            period = 300
            stat   = "Sum"
            view   = "timeSeries"
          }
        },
        {
          type   = "metric"
          x      = 16
          y      = 24
          width  = 8
          height = 6
          properties = {
            title  = "DynamoDB Throttled Requests"
            region = data.aws_region.current.name
            metrics = [
              [
                "AWS/DynamoDB", "ThrottledRequests",
                "TableName", var.registry_table_name
              ]
            ]
            period = 300
            stat   = "Sum"
            view   = "timeSeries"
          }
        }
      ]
    )
  })
}
