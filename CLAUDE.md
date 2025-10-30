# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This repository contains a GLM API proxy demo with two main components:

1. **Python GLM Client** (`main.py`, `glm_client.py`) - A simple synchronous streaming client for GLM Flash API
2. **Rust DeepSeek Proxy** (`deepseek_proxy/`) - A secure API proxy service with authentication, rate limiting, and quota management

## Commands

### Python Client
```bash
# Set GLM API key
$env:GLM_FLASH_API_KEY = "your-api-key"

# Run GLM client demo
uv run python main.py
```

### Rust Proxy Service
```bash
# Development - start proxy service
cd deepseek_proxy
.\start.ps1

# Alternative development run
cd deepseek_proxy
cargo run

# Production build
cd deepseek_proxy
cargo build --release

# Test the proxy service
cd deepseek_proxy
uv run python test_proxy.py

# Test DeepSeek API directly
cd deepseek_proxy
uv run python test_deepseek.py
```

### Environment Setup
```bash
# Set DeepSeek API key (for proxy service)
$env:OPENAI_API_KEY = "your-deepseek-api-key"

# Enable debug logging for Rust proxy
$env:RUST_LOG = "deepseek_proxy=debug,tower_http=debug"
```

## Architecture

### GLM Client (`glm_client.py`)
- **Purpose**: Direct synchronous streaming client for GLM Flash API
- **Key Feature**: Context manager pattern with automatic client cleanup
- **API**: Smart Spectrum GLM-4.5-Flash model via `https://open.bigmodel.cn/api/paas/v4`
- **Environment**: Reads API key from `GLM_FLASH_API_KEY`

### DeepSeek Proxy Service (`deepseek_proxy/`)
A comprehensive Rust-based API proxy with enterprise-grade features:

#### Core Modules:
- **`src/main.rs`**: Application entry point with unified AppState
- **`src/auth/`**: JWT-based authentication system with login caching
- **`src/proxy/`**: Request forwarding with token-based rate limiting  
- **`src/deepseek/`**: DeepSeek API client with streaming support
- **`src/quota/`**: Monthly quota management with lazy persistence
- **`src/config.rs`**: Configuration management via TOML

#### Key Features:
1. **Security**: API key hiding, JWT tokens, user authentication
2. **Rate Limiting**: 
   - Login caching (60s within same user)
   - Token serialization (1 request per token simultaneously)
   - Multi-user concurrency support
3. **Quota Management**: Tiered monthly quotas (basic/pro/premium) with persistent tracking
4. **Graceful Shutdown**: Ctrl+C saves quota data before exit

#### State Management:
```rust
pub struct AppState {
    pub config: Arc<Config>,
    pub jwt_service: Arc<JwtService>,
    pub deepseek_client: Arc<DeepSeekClient>,
    pub token_limiter: Arc<TokenLimiter>,
    pub login_limiter: Arc<LoginLimiter>,
    pub quota_manager: Arc<QuotaManager>,
}
```

#### API Endpoints:
- `POST /auth/login` - User authentication (returns JWT token)
- `POST /chat/completions` - Proxy to DeepSeek API (requires Bearer token)
- `GET /auth/quota` - Query user quota status

#### Configuration:
- **Users**: Defined in `config.toml` with username/password/quota_tier
- **Quota Tiers**: basic (500/month), pro (1000/month), premium (1500/month)
- **Persistence**: JSON files in `data/quotas/` with lazy write (every 100 requests)

## Development Patterns

### Error Handling
The Rust proxy uses comprehensive error types with proper HTTP status codes:
- `401` - Invalid/expired token
- `402` - Monthly quota exceeded
- `403` - Account disabled
- `429` - Token concurrent request limit
- `502/504` - Upstream API errors

### Concurrency Model
- **Axum + Tokio**: Async web framework with efficient request handling
- **Semaphore-based**: Token-level serialization prevents request conflicts
- **Arc<Mutex>**: Thread-safe shared state for caches and limiters

### Configuration Management
- Environment variables for API keys (`GLM_FLASH_API_KEY`, `OPENAI_API_KEY`)
- TOML files for application config (`config.toml`)
- PowerShell scripts for convenience (`start.ps1`, `set_api_key.ps1`)

## Testing

### Proxy Service Testing
```bash
cd deepseek_proxy
uv run python test_proxy.py
```

Test scenarios covered:
1. Login authentication and caching
2. Streaming chat completions
3. Token-based rate limiting
4. Multi-user concurrency
5. Quota enforcement (if implemented)
6. Unauthorized access prevention

### Manual Testing
```bash
# Login to get token
curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "admin123"}'

# Use token for chat
curl -X POST http://localhost:8080/chat/completions \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model": "deepseek-chat", "messages": [{"role": "user", "content": "Hello"}], "stream": true}'
```

## Important Notes

### Security Considerations
- Never commit API keys to repository
- Change default JWT secret in production
- API keys should be set via environment variables only
- User credentials in `config.toml` are for demo purposes

### Data Persistence
- Quota data is stored in `deepseek_proxy/data/quotas/`
- Lazy write strategy: saves every 100 requests or on graceful shutdown
- Monthly quotas reset automatically on the 1st of each month

### Dependencies
- **Python**: httpx for HTTP client functionality
- **Rust**: Comprehensive web stack (axum, tokio, reqwest, serde, jsonwebtoken)
- **Build Tools**: uv for Python package management, cargo for Rust

## Project Files Structure

```
├── main.py                     # GLM client demo
├── glm_client.py              # GLM API client implementation  
├── pyproject.toml             # Python project configuration
├── deepseek_proxy/            # Rust proxy service
│   ├── src/
│   │   ├── main.rs           # App entry with unified state
│   │   ├── auth/             # JWT authentication module
│   │   ├── proxy/            # Request forwarding + rate limiting
│   │   ├── deepseek/         # DeepSeek API client
│   │   ├── quota/            # Monthly quota management
│   │   └── config.rs         # Configuration management
│   ├── config.toml           # Service configuration
│   ├── start.ps1             # Development startup script
│   └── data/quotas/          # Quota persistence directory
├── QUOTA_DESIGN.md           # Detailed quota system design
├── RUST_PROXY_DESIGN.md      # Comprehensive proxy architecture
└── README.md                 # User-facing documentation
```