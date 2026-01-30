output "mqtt_nlb_dns" {
  description = "DNS name of the MQTT Network Load Balancer"
  value       = aws_lb.mqtt.dns_name
}

output "api_alb_dns" {
  description = "DNS name of the registry Application Load Balancer"
  value       = aws_lb.api.dns_name
}

output "mqtt_target_group_arn" {
  description = "ARN of the MQTT target group (used by ECS service)"
  value       = aws_lb_target_group.mqtt.arn
}

output "registry_target_group_arn" {
  description = "ARN of the registry target group (used by ECS service)"
  value       = aws_lb_target_group.registry.arn
}

output "api_alb_arn" {
  description = "Full ARN of the registry ALB"
  value       = aws_lb.api.arn
}

output "api_alb_arn_suffix" {
  description = "ARN suffix of the ALB (for CloudWatch metrics)"
  value       = aws_lb.api.arn_suffix
}

output "mqtt_nlb_arn_suffix" {
  description = "ARN suffix of the NLB (for CloudWatch metrics)"
  value       = aws_lb.mqtt.arn_suffix
}

output "mqtt_target_group_arn_suffix" {
  description = "ARN suffix of the MQTT target group (for CloudWatch metrics)"
  value       = aws_lb_target_group.mqtt.arn_suffix
}

output "registry_target_group_arn_suffix" {
  description = "ARN suffix of the registry target group (for CloudWatch metrics)"
  value       = aws_lb_target_group.registry.arn_suffix
}

output "nes_coordinator_target_group_arn" {
  description = "ARN of the NES coordinator target group (used by ECS service)"
  value       = aws_lb_target_group.nes_coordinator.arn
}

output "mqtt_ws_target_group_arn" {
  description = "ARN of the MQTT WebSocket target group (used by ECS service)"
  value       = aws_lb_target_group.mqtt_ws.arn
}

output "nes_nlb_dns" {
  description = "DNS name of the internal NES coordinator NLB"
  value       = aws_lb.nes_coordinator.dns_name
}

output "nes_nlb_rest_target_group_arn" {
  description = "ARN of the NES REST target group (NLB port 8081)"
  value       = aws_lb_target_group.nes_rest.arn
}

output "nes_nlb_grpc_target_group_arn" {
  description = "ARN of the NES gRPC target group (NLB port 8080)"
  value       = aws_lb_target_group.nes_grpc.arn
}

output "nes_nlb_worker_rpc_target_group_arn" {
  description = "ARN of the NES worker RPC target group (NLB port 4000)"
  value       = aws_lb_target_group.nes_worker_rpc.arn
}

output "nes_nlb_worker_data_target_group_arn" {
  description = "ARN of the NES worker data target group (NLB port 4001)"
  value       = aws_lb_target_group.nes_worker_data.arn
}

output "nes_nlb_zone_id" {
  description = "Route53 zone ID of the internal NES NLB (for alias records)"
  value       = aws_lb.nes_coordinator.zone_id
}
