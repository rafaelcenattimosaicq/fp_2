terraform {
  backend "s3" {
    bucket         = "iot-platform-tfstate-451633548946"
    key            = "infra/terraform.tfstate"
    region         = "us-east-1"
    dynamodb_table = "iot-platform-tflock"
    encrypt        = true
  }
}
