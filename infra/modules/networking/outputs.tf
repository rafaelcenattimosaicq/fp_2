output "vpc_id" {
  description = "ID of the VPC"
  value       = aws_vpc.main.id
}

output "public_subnet_ids" {
  description = "IDs of the two public subnets (for ALB, NAT gateway)"
  value       = aws_subnet.public[*].id
}

output "private_subnet_ids" {
  description = "IDs of the two private subnets (for ECS Fargate tasks)"
  value       = aws_subnet.private[*].id
}

output "alb_security_group_id" {
  description = "Security group ID for the Application Load Balancer"
  value       = aws_security_group.alb.id
}

output "mqtt_broker_security_group_id" {
  description = "Security group ID for the MQTT broker (NLB target)"
  value       = aws_security_group.mqtt_broker.id
}

output "registry_security_group_id" {
  description = "Security group ID for the registry service"
  value       = aws_security_group.registry.id
}

output "internal_security_group_id" {
  description = "Security group ID for internal service-to-service traffic"
  value       = aws_security_group.internal.id
}
