# CQUPT ICS Calendar Generator

一个用于重庆邮电大学课程表的 ICS 日历生成工具，本项目受到 [CQUPT ICS Python](https://github.com/qwqVictor/CQUPT-ics)项目的启发。

## 项目概述

该项目提供了三种使用方式：
- **核心库 (cqupt-ics-core)**: 提供课程数据获取和 ICS 生成的核心功能
- **命令行工具 (cqupt-ics-cli)**: 终端命令行接口，适合脚本化使用
- **Web 服务 (cqupt-ics-server)**: HTTP API 服务，支持远程调用

## 系统要求

- Rust 2024 Edition

## 安装说明

### 从源码构建

```bash
# 克隆项目
git clone https://github.com/jizizr/cqupt-ics-rs.git
cd cqupt-ics-rs

# 构建所有组件
cargo build --release

# 或者构建特定组件
cargo build --release --bin cqupt-ics    # 命令行工具
cargo build --release --bin server       # Web 服务
```

### 使用 Docker

```bash
# 构建自定义镜像
docker build -t cqupt-ics .
# 使用 Docker Compose 运行服务
docker-compose up -d
```

## 使用方法

### 命令行工具

```bash
# 基本用法
./target/release/cqupt-ics generate \
  --provider redrock \
  --username your_student_id \
  --password your_password \
  --output schedule.ics

# 指定学期
./target/release/cqupt-ics generate \
  --provider redrock \
  --username your_student_id \
  --password your_password \
  --year 2024 \
  --term 1 \
  --output schedule.ics

# 自定义选项
./target/release/cqupt-ics generate \
  --provider redrock \
  --username your_student_id \
  --password your_password \
  --calendar-name "我的课程表" \
  --timezone "Asia/Shanghai" \
  --reminder 15 \
  --output schedule.ics
```

### Web 服务

启动服务：

```bash
# 设置 Redis 连接
export REDIS_URL="redis://localhost:6379"

# 启动服务
./target/release/server
```

API 调用示例：

```bash
# 生成课程表
http://localhost:3000/courses?provider=redrock&username=2023214567&password=684104
# 查看支持的数据源
http://localhost:3000/api/providers

# 查看位置映射
http://localhost:3000/api/locations
```

### 核心库集成

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
cqupt-ics-core = { path = "cqupt-ics-core" }
tokio = { version = "1.0", features = ["full"] }
```

## 支持的数据源

| 数据源      | 标识符    | 说明                                             |
| ----------- | --------- | ------------------------------------------------ |
| Redrock API | `redrock` | 重庆邮电大学红岩网校开发的「掌上重邮」app 数据源 |
| Wecqupt API | `wecqupt` | 重庆邮电大学「We 重邮」微信小程序数据源          |
## 配置选项

### ICS 生成选项

- `calendar_name`: 日历名称
- `timezone`: 时区设置（默认: Asia/Shanghai）
- `reminder_minutes`: 提醒时间（分钟）
- `include_description`: 是否包含课程描述
- `include_exam`: 是否包含考试安排

### 环境变量

- `REDIS_URL`: Redis 连接字符串（仅服务端）
- `RUST_LOG`: 日志级别设置

## 开发说明

### 项目结构

```
├── cqupt-ics-core/      # 核心功能库
│   ├── src/
│   │   ├── providers/   # 数据源实现
│   │   ├── ics/         # ICS 生成
│   │   ├── cache/       # 缓存机制
│   │   └── types/       # 数据类型定义
├── cqupt-ics-cli/       # 命令行工具
├── cqupt-ics-server/    # Web 服务
└── target/              # 构建输出
```

### 添加新数据源

1. 在 `cqupt-ics-core/src/providers/` 创建新的 provider 文件
2. 实现 `Provider` trait
```rust
#[async_trait]
pub trait Provider: Send + Sync {
    /// Token type for this provider
    type Token: Send + Sync + Serialize + DeserializeOwned;
    type ContextType: Send + Sync;
    /// Provider 的名字
    fn name(&self) -> &str;

    /// Provider 的描述
    fn description(&self) -> &str;

    /// Get timezone for this provider
    ///
    /// Returns the timezone used by this provider for time calculations.
    /// This is used to ensure consistent timezone handling across all
    /// provider operations.
    /// provider 的时区
    fn timezone(&self) -> FixedOffset;

    /// Authenticate and get token
    /// 获取 token 的方法
    async fn authenticate<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &CourseRequest,
    ) -> Result<Self::Token>;

    /// Validate existing token
    /// 验证 token 是否有效
    async fn validate_token(&self, token: &Self::Token) -> Result<bool>;

    /// Refresh token
    /// 刷新 token 的方法（如果 provider 不存在则直接返回 Err 即可）
    async fn refresh_token(&self, token: &Self::Token) -> Result<Self::Token>;

    /// Get courses using token
    /// request.semester should be Some before calling this method
    /// If use crate::providers::Wrapper, it will ensure semester is Some
    /// 获取课程数据的方法
    async fn get_courses<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<CourseResponse>;

    /// Get semester start date
    /// This is called if request.semester is None before get_courses
    /// If you use crate::providers::Wrapper, it will call this method automatically if request.semester is None
    /// You can use the context to store intermediate data if needed
    /// 获取学期开始日期的方法
    async fn get_semester_start<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<chrono::DateTime<FixedOffset>>;

    /// Token TTL
    /// 返回 token 的有效期，用于控制缓存
    fn token_ttl(&self) -> Duration {
        Duration::from_secs(3600 * 24) // 24 hours default
    }
}
```
3. 在 `registry.rs` 中注册新的 provider

## 贡献指南

1. Fork 本项目
2. 创建功能分支
3. 提交更改并添加测试
4. 确保所有测试通过
5. 提交 Pull Request

## 许可协议

本项目采用 MIT 许可协议。详见 [LICENSE](LICENSE) 文件。

## 相关链接
- [iCalendar 规范 (RFC 5545)](https://tools.ietf.org/html/rfc5545)
- [CQUPT ICS Python版本项目](https://github.com/qwqVictor/CQUPT-ics)

## 问题反馈

如遇到问题或有改进建议，请通过 [GitHub Issues](https://github.com/jizizr/cqupt-ics-rs/issues) 联系我们。