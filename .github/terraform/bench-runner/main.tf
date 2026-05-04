terraform {
  backend "s3" {
    region = "us-west-2"
    bucket = "gear-terraform"
    key    = "bench-runner/terraform.tfstate"
  }
}

variable "aws_region" {
  type    = string
  default = "us-west-2"
}
variable "instance_type" {
  type    = string
  default = "t3.micro"
}
variable "max_cpu_frequency" {
  type    = string
  default = 6000000
}
variable "instance_disk_size" {
  type    = string
  default = 30
}
variable "registration_token" {
  type    = string
  default = ""
}
variable "github_run_id" {
  type    = string
  default = ""
}

provider "aws" {
  region = var.aws_region
}

data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"]

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*"]
  }
}

data "aws_vpc" "default" {
  default = true
}

data "aws_security_group" "default" {
  vpc_id = data.aws_vpc.default.id
  filter {
    name   = "group-name"
    values = ["default"]
  }
}

data "aws_subnets" "default" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.default.id]
  }
}

resource "aws_instance" "bench_runner" {
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = var.instance_type
  key_name               = "root"
  subnet_id              = data.aws_subnets.default.ids[0]
  vpc_security_group_ids = [data.aws_security_group.default.id]

  root_block_device {
    volume_type           = "gp3"
    volume_size           = var.instance_disk_size
    delete_on_termination = true
  }

  user_data = <<-EOF
                #!/bin/bash
                cpupower frequency-set --governor performance
                cpupower frequency-set --max ${var.max_cpu_frequency}
                echo never > /sys/kernel/mm/transparent_hugepage/enabled
                echo never > /sys/kernel/mm/transparent_hugepage/defrag
                echo 0 > /proc/sys/vm/nr_hugepages
                mkdir /runner
                chown ubuntu:ubuntu -R /runner
                apt update
                apt install -y jq docker.io
                systemctl enable --now docker
                usermod -aG docker ubuntu

                mkdir -p /home/ubuntu/.ssh
                chmod 700 /home/ubuntu/.ssh
                echo "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGd8nO78hzAVTjGaW6IJgFnI32qY/vCpQRpO8lW97eXA root" >> /home/ubuntu/.ssh/authorized_keys
                chown -R ubuntu:ubuntu /home/ubuntu/.ssh
                chmod 600 /home/ubuntu/.ssh/authorized_keys

                sudo -u ubuntu -i bash -c "
                cd /runner &&
                curl -o actions-runner-linux-x64.tar.gz -L `curl -s https://api.github.com/repos/actions/runner/releases/latest | jq -r '.assets[] | select(.name | contains(\"actions-runner-linux-x64\")) | .browser_download_url'` &&
                tar xzf actions-runner-linux-x64.tar.gz &&
                sudo ./bin/installdependencies.sh &&
                ./config.sh --name bench-runner --runnergroup default --no-default-labels --labels bench --replace --work _work --url https://github.com/gear-tech/gear --token ${var.registration_token} &&
                ./run.sh
                "
              EOF

  tags = {
    Name = "bench-runner-${var.github_run_id}"
  }

  timeouts {
    create = "30m"
    delete = "60m"
  }
}

output "instance_region" {
  value       = var.aws_region
}
output "instance_name" {
  value       = aws_instance.bench_runner.tags["Name"]
}
output "instance_type" {
  value       = var.instance_type
}
output "max_cpu_frequency" {
  value       = "${var.max_cpu_frequency} MHz"
}
output "instance_public_ip" {
  value       = aws_instance.bench_runner.public_ip
}
