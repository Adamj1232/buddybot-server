# BuddyBot Server

A secure websocket server for BuddyBot that proxies requests to Claude API, providing authentication, connection management, and auto-scaling capabilities.

## Features

- Secure websocket connections for real-time communication
- Authentication via uAuth
- API key obfuscation for Claude API requests
- Auto-scaling based on CPU usage
- Connection management with heartbeat detection
- Strong security measures

## Development

### Prerequisites

- Rust 1.72+
- Docker and Docker Compose
- PostgreSQL

### Setup

1. Clone the repository
2. Create a `.env` file with the required environment variables
3. Run `docker-compose up -d` to start the development environment
4. Run `cargo run` to start the server

## Architecture

The server is built with a modular architecture:

- **Auth Module**: Handles user authentication and session management
- **WebSocket Module**: Manages client connections
- **Proxy Module**: Handles Claude API requests
- **Scaling Module**: Manages auto-scaling based on CPU usage
- **Database Module**: Manages data persistence

## Deployment

For production deployment, the server can be containerized and deployed to a container orchestration platform like Kubernetes.