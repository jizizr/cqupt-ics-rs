# 多阶段构建的 Dockerfile for CQUPT-ics Server

# 构建阶段
FROM rust:slim-bookworm AS builder

# 安装构建依赖
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 设置工作目录
WORKDIR /app

# 复制 Cargo 文件
COPY Cargo.toml Cargo.lock ./
COPY cqupt-ics-core/Cargo.toml ./cqupt-ics-core/
COPY cqupt-ics-server/Cargo.toml ./cqupt-ics-server/
COPY cqupt-ics-cli/Cargo.toml ./cqupt-ics-cli/

# 创建源代码的 dummy 文件以便缓存依赖
RUN mkdir -p cqupt-ics-core/src cqupt-ics-server/src cqupt-ics-cli/src \
    && echo "fn main() {}" > cqupt-ics-core/src/lib.rs \
    && echo "fn main() {}" > cqupt-ics-server/src/main.rs \
    && echo "fn main() {}" > cqupt-ics-cli/src/main.rs

# 预构建依赖以利用 Docker 缓存
RUN cargo build --release --bin server

# 复制实际源代码
COPY cqupt-ics-core/src ./cqupt-ics-core/src/
COPY cqupt-ics-server/src ./cqupt-ics-server/src/

# 构建实际的 server 二进制文件
RUN touch cqupt-ics-core/src/lib.rs cqupt-ics-server/src/main.rs \
    && cargo build --release --bin server

# 运行阶段
FROM debian:bookworm-slim

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 创建非 root 用户
RUN groupadd -r cqupt && useradd -r -g cqupt -s /bin/false cqupt

# 设置工作目录
WORKDIR /app

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/server /app/server

# 更改所有权并设置权限
RUN chown cqupt:cqupt /app/server && chmod +x /app/server

# 切换到非 root 用户
USER cqupt

# 暴露端口
EXPOSE 3000

# 设置环境变量
ENV RUST_LOG=info
ENV PORT=3000

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:${PORT}/health || exit 1

# 启动服务
CMD ["./server"]