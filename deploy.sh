#!/bin/bash

# CQUPT ICS Server 部署脚本

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印带颜色的消息
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 检查 Docker 是否安装
check_docker() {
    if ! command -v docker &> /dev/null; then
        print_error "Docker 未安装，请先安装 Docker"
        exit 1
    fi

    if ! command -v docker-compose &> /dev/null; then
        print_error "Docker Compose 未安装，请先安装 Docker Compose"
        exit 1
    fi

    print_success "Docker 和 Docker Compose 已安装"
}

# 构建镜像
build_image() {
    print_info "构建 CQUPT ICS Server 镜像..."
    docker build -t cqupt-ics-server:latest .
    
    if [ $? -eq 0 ]; then
        print_success "镜像构建成功"
    else
        print_error "镜像构建失败"
        exit 1
    fi
}

# 停止现有服务
stop_services() {
    print_info "停止现有服务..."
    docker-compose down 2>/dev/null || true
    print_success "服务已停止"
}

# 启动服务
start_services() {
    local compose_file=${1:-"docker-compose.yml"}
    print_info "使用 ${compose_file} 启动服务..."
    
    docker-compose -f "$compose_file" up -d
    
    if [ $? -eq 0 ]; then
        print_success "服务启动成功"
        print_info "等待服务健康检查..."
        sleep 10
        
        # 检查服务状态
        if docker-compose -f "$compose_file" ps | grep -q "Up (healthy)"; then
            print_success "服务运行正常"
            print_info "服务地址: http://localhost:3000"
            print_info "健康检查: http://localhost:3000/health"
        else
            print_warning "服务可能还在启动中，请稍后检查"
        fi
    else
        print_error "服务启动失败"
        exit 1
    fi
}

# 显示日志
show_logs() {
    local compose_file=${1:-"docker-compose.yml"}
    print_info "显示服务日志..."
    docker-compose -f "$compose_file" logs -f
}

# 清理资源
cleanup() {
    print_info "清理 Docker 资源..."
    docker system prune -f
    print_success "清理完成"
}

# 显示帮助信息
show_help() {
    echo "CQUPT ICS Server 部署脚本"
    echo ""
    echo "用法: $0 [选项]"
    echo ""
    echo "选项:"
    echo "  build     构建 Docker 镜像"
    echo "  start     启动开发环境服务 (默认)"
    echo "  prod      启动生产环境服务"
    echo "  stop      停止服务"
    echo "  restart   重启服务"
    echo "  logs      显示服务日志"
    echo "  logs-dev  显示开发环境日志"
    echo "  logs-prod 显示生产环境日志"
    echo "  status    显示服务状态"
    echo "  cleanup   清理 Docker 资源"
    echo "  help      显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 build && $0 start     # 构建并启动开发环境"
    echo "  $0 prod                  # 启动生产环境"
    echo "  $0 logs                  # 查看日志"
}

# 显示服务状态
show_status() {
    print_info "服务状态:"
    docker-compose ps 2>/dev/null || print_warning "没有运行的服务"
    
    if docker-compose ps | grep -q "Up"; then
        print_info "测试连接..."
        if curl -sf http://localhost:3000/health >/dev/null 2>&1; then
            print_success "服务运行正常 ✓"
        else
            print_warning "服务可能还在启动中"
        fi
    fi
}

# 主逻辑
main() {
    case "${1:-start}" in
        "build")
            check_docker
            build_image
            ;;
        "start")
            check_docker
            stop_services
            start_services "docker-compose.yml"
            ;;
        "prod")
            check_docker
            stop_services
            start_services "docker-compose.prod.yml"
            ;;
        "stop")
            stop_services
            ;;
        "restart")
            check_docker
            stop_services
            start_services "docker-compose.yml"
            ;;
        "logs")
            show_logs "docker-compose.yml"
            ;;
        "logs-dev")
            show_logs "docker-compose.yml"
            ;;
        "logs-prod")
            show_logs "docker-compose.prod.yml"
            ;;
        "status")
            show_status
            ;;
        "cleanup")
            cleanup
            ;;
        "help"|"-h"|"--help")
            show_help
            ;;
        *)
            print_error "未知选项: $1"
            show_help
            exit 1
            ;;
    esac
}

# 运行主逻辑
main "$@"