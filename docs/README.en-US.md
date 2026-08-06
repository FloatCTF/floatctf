<h1 align="center">
  <img src="./float.png" alt="FloatCTF" width="128" />
  <br>
  FloatCTF
  <br>
</h1>

<h3 align="center">
A CTF Platform based on <a href="https://rust-lang.org/">Rust</a>.
</h3>

![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
[![Actix Web](https://img.shields.io/badge/Actix_Web-000000?logo=actix&logoColor=white)](https://actix.rs/)
[![SeaORM](https://img.shields.io/badge/SeaORM-222222?logo=rust&logoColor=white)](https://www.sea-ql.org/SeaORM/)
[![Zed](https://img.shields.io/badge/Zed-084CCF?logo=zed&logoColor=white)](https://zed.dev/)
![React](https://img.shields.io/badge/React-20232a.svg?logo=react&logoColor=61DAFB)
![tailwindcss](https://img.shields.io/badge/tailwindcss-38B2AC.svg?logo=tailwind-css&logoColor=white)
[![TanStack Router](https://img.shields.io/badge/TanStack_Router-FF4154?logo=react-router&logoColor=white)](https://tanstack.com/router)
[![TanStack Query](https://img.shields.io/badge/TanStack_Query-FF4154?logo=react-query&logoColor=white)](https://tanstack.com/query)

[中文](./README.md) | English

## Star History

<a href="https://www.star-history.com/?repos=FloatCTF%2Ffloatctf&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=FloatCTF/floatctf&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=FloatCTF/floatctf&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=FloatCTF/floatctf&type=date&legend=top-left" />
 </picture>
</a>

## Table of Contents

- [Overview](#overview)
- [Repositories](#repositories)
- [Architecture](#architecture)
- [Requirements](#requirements)
- [Quick Install](#quick-install)
- [Getting Started](#getting-started)
  - [1. Clone the Project](#1-clone-the-project)
  - [2. Configure Environment Variables](#2-configure-environment-variables)
  - [3. Initialize the Platform](#3-initialize-the-platform)
  - [4. Start Services](#4-start-services)
  - [5. Access the Platform](#5-access-the-platform)
- [Screenshots](#screenshots)
  - [User Interface](#user-interface)
  - [Admin Interface](#admin-interface)
- [Core Features](#core-features)
  - [User Features](#user-features)
  - [Admin Features](#admin-features)
- [AWD Attack-Defense](#awd-attack-defense)
- [Tech Stack](#tech-stack)
- [Highlights](#highlights)
- [Directory Structure](#directory-structure)
- [Service Overview](#service-overview)
- [Common Commands](#common-commands)
- [Troubleshooting](#troubleshooting)
- [License](#license)

## Overview

An open-source CTF training and competition platform built with Rust.

## Repositories

FloatCTF uses a multi-repo architecture with independently maintained components:

| Repository                                                             | Description                                    |
| ---------------------------------------------------------------------- | ---------------------------------------------- |
| **[floatctf](https://github.com/FloatCTF/floatctf)**                   | Main repo / Landing page / Docker deploy       |
| [floatctf-api](https://github.com/FloatCTF/floatctf-api)               | Backend API (Rust / Actix Web)                 |
| [floatctf-web](https://github.com/FloatCTF/floatctf-web)               | Frontend (React)                               |
| [floatctf-develop](https://github.com/FloatCTF/floatctf-develop)       | Dev environment (DevContainer)                 |
| [floatctf-installer](https://github.com/FloatCTF/floatctf-installer)   | Host installation script                       |
| [floatctf-challenges](https://github.com/FloatCTF/floatctf-challenges) | Challenge repository                           |
| [challenge-template](https://github.com/FloatCTF/challenge-template)   | Challenge tutorial / template                  |
| [fcmc](https://github.com/FloatCTF/fcmc)                               | Container management / challenge tooling       |
| [floatctf-challenge-creator](https://github.com/FloatCTF/floatctf-challenge-creator) | Claude Code challenge creation Skill |

**Events:**

| Repository                                                                                           | Description              |
| ---------------------------------------------------------------------------------------------------- | ------------------------ |
| [challenges-xxxxxxxx-xxxxx-template](https://github.com/FloatCTF/challenges-xxxxxxxx-xxxxx-template) | Event template           |
| [challenges-202510-freshcup](https://github.com/FloatCTF/challenges-202510-freshcup)                 | Past event challenges    |

## Architecture

The platform consists of 4 core services:

| Service           | Image             | Description                          |
| ----------------- | ----------------- | ------------------------------------ |
| `floatctf-db`     | PostgreSQL 17     | Database                             |
| `floatctf-rustfs` | rustfs/rustfs     | S3-compatible object storage         |
| `floatctf-nginx`  | Nginx 1.26        | Reverse proxy & static file serving  |
| `floatctf-api`    | Alpine + floatctf | Backend API                          |

## Requirements

- Docker and Docker Compose
- ~10 GB free disk space

## Quick Install

```bash
S=/tmp/ifctf; curl -sL https://github.com/FloatCTF/floatctf/raw/refs/heads/main/install.sh >$S && vim $S && bash $S; rm $S
```

## Getting Started

### 1. Clone the Project

```bash
git clone https://github.com/FloatCTF/floatctf.git
cd floatctf
```

### 2. Configure Environment Variables

Edit the `.env` configuration file:

```env
API_ELF_URL="https://github.com/FloatCTF/floatctf/releases/latest/download/floatctf-linux-amd64-musl"
SQL_DIST_URL="https://github.com/FloatCTF/floatctf/releases/latest/download/sql.tar.gz"
HTML_DIST_URL="https://github.com/FloatCTF/floatctf-web/releases/latest/download/download/html.tar.gz"

INSTALLER_DIR="./app"

# API CONF
API_SERVER_IP="floatctf-api"
API_SERVER_PORT=9090
API_USER="floatctf_api"
NODE_IP="127.0.0.1"

# nginx
NGINX_SERVER_HTTP_PORT=80
NGINX_SERVER_HTTPS_PORT=443
NGINX_USER="nginx"

# database
PG_HOST="floatctf-db"
PG_PORT=5432
PG_USER=postgres
PG_PASSWORD=postgres
PG_DB=floatctf_db

# rustfs
RUSTFS_ACCESS_KEY=rustfsadmin
RUSTFS_SECRET_ACCESS_KEY=rustfsadmin
RUSTFS_ADDRESS="floatctf-rustfs:9000"

DOCKER_HOST_PATH="/var/run/docker.sock"
```

### 3. Initialize the Platform

```bash
chmod +x init.sh
./init.sh
```

The initialization script will:

- Create required directories
- Generate self-signed SSL certificates
- Download the API binary, SQL schema, and frontend files
- Configure environment variable files
- Set up Nginx configuration

### 4. Start Services

```bash
docker compose --env-file ./.env --env-file ./app/.env up -d
```

### 5. Access the Platform

- Web UI: `https://localhost:9443`
- API: `https://localhost:9443/api/`

## Screenshots

### User Interface

|            Login            |         Leaderboard         |           Challenges            |
| :-------------------------: | :-------------------------: | :-----------------------------: |
| ![Login](docs/images/login.png) | ![Home](docs/images/home.png) | ![Challenges](docs/images/challenges.png) |

|           Discussion            |          Scoreboard           |
| :-----------------------------: | :----------------------------: |
| ![Discussion](docs/images/discussion.png) | ![Scoreboard](docs/images/scoreboard.png) |

|        Event Challenges         |
| :-----------------------------: |
| ![Event Challenges](docs/images/event_challenges.png) |

### Admin Interface

|           Dashboard            |
| :----------------------------: |
| ![Dashboard](docs/images/dashboard.png) |

|       Challenge Management       |          Score Overview           |
| :-------------------------------: | :-------------------------------: |
| ![Challenge Management](docs/images/event_detail.png) | ![Score Overview](docs/images/score.png) |

## Core Features

### User Features

- **Auth** — Student ID registration, JWT token authentication, Argon2 password hashing, password reset
- **Leaderboard** — Real-time solve rankings to fuel long-term self-driven learning
- **Challenges** — Browse by category (Web / Pwn / Crypto / Reverse / Misc); one-click start spins up an isolated Docker container; instructors can publish curated "problem sets" for targeted training
- **Discussion** — Built-in forum for peer-to-peer learning and knowledge sharing
- **Competition / Scoreboard** — Supports Jeopardy and AWD (Attack-Defense) formats; real-time scoreboard, trend charts, first-blood markers, and event announcements

### Admin Features

- **Dashboard** — Visual system status panel with real-time server load, memory/disk usage, and network traffic
- **Event Management** — CRUD for challenges, Docker image configuration, port mapping, attachment uploads, scoring rule configuration
- **Logs** — Operation logs and audit trail queries
- **Docker** — View all running container instances; force-destroy abnormal containers to free resources
- **Tasks** — Task queue management and scheduling

## AWD Attack-Defense

AWD (Attack With Defense) is the platform's signature feature. A hybrid network architecture built with Docker custom bridges and WireGuard VPN assigns each team an isolated virtual subnet, ensuring environment isolation and traffic control. Contestants connect to the competition intranet via WireGuard clients to attack opponent machines and defend their own, replicating real-world intranet attack-defense scenarios.

## Tech Stack

| Module       | Technology                              | Description                              |
| ------------ | --------------------------------------- | ---------------------------------------- |
| Language     | Rust                                    | Systems language with compile-time memory safety |
| Web Framework| Actix Web                               | Async high-concurrency web framework     |
| ORM          | SeaORM                                  | Type-safe async ORM                      |
| Database     | PostgreSQL 17                           | Relational database                      |
| Object Store | RustFS                                  | S3-compatible object storage             |
| Frontend     | React + TanStack Query + Primer Design  | Smooth interactive experience            |
| Containers   | Docker / Docker Compose                 | Challenge isolation & deployment         |
| VPN          | WireGuard                               | AWD competition network isolation        |
| Auth         | JWT + Argon2                            | Token auth + strong password hashing     |
| Reverse Proxy| Nginx 1.26                              | Static file serving & API proxying       |

## Highlights

- **High Performance** — Rust + Actix Web async architecture keeps API response latency under 100ms even with hundreds of concurrent flag submissions
- **Secure & Reliable** — Rust's ownership model eliminates memory safety issues at compile time; multi-layered protection with JWT, Argon2, and container resource limits
- **Environment Isolation** — Each challenge runs in its own Docker container with instant startup and auto-timeout recycling; WireGuard subnet isolation in AWD mode
- **Dynamic Scoring** — Square-root-based score decay algorithm where points drop non-linearly with solve count, balancing differentiation and fairness
- **One-Click Deploy** — Docker Compose orchestrates all services for quick migration and standardized deployment

## Directory Structure

```
floatctf/
├── docker-compose.yml    # Service orchestration config
├── init.sh              # Initialization script
├── install.sh           # One-click install script
├── .env                 # Environment variables
├── README.md
├── LICENSE
├── .gitignore
└── app/
    ├── .env             # Runtime environment variables
    ├── bin/             # floatctf API binary
    ├── html/            # Frontend static files
    ├── data/            # RustFS data volume
    ├── keys/            # SSL certificates
    ├── logs/            # Application logs
    │   ├── api/
    │   ├── nginx/
    │   └── rustfs/
    ├── nginx/conf/      # Nginx configuration
    ├── tmp/             # Temporary files
    │   └── sql/         # Database schema
    └── 1.py             # S3 bucket init script
```

## Service Overview

| Service           | Image             | Ports           | Description                                                                   |
| ----------------- | ----------------- | --------------- | ----------------------------------------------------------------------------- |
| `floatctf-db`     | PostgreSQL 17     | 5432 (internal) | Database; persistent volume `pgdata`                                          |
| `floatctf-rustfs` | rustfs/rustfs     | 9000 (internal) | S3-compatible object storage; `floatctf-public` (assets), `floatctf-private` (writeups) |
| `floatctf-nginx`  | Nginx 1.26        | 9980 / 9443     | Reverse proxy & static file serving                                          |
| `floatctf-api`    | Alpine + floatctf | —               | Backend API; connects to PostgreSQL and RustFS                                |

## Common Commands

```bash
# View logs
docker compose --env-file ./.env --env-file ./app/.env logs -f

# View specific service logs
docker compose --env-file ./.env --env-file ./app/.env logs -f floatctf-api
docker compose --env-file ./.env --env-file ./app/.env logs -f floatctf-nginx

# Restart services
docker compose --env-file ./.env --env-file ./app/.env restart

# Stop services
docker compose --env-file ./.env --env-file ./app/.env down

# Rebuild and restart
docker compose --env-file ./.env --env-file ./app/.env up -d --force-recreate
```

## Troubleshooting

| Issue                    | Diagnosis                                                                                          |
| ------------------------ | -------------------------------------------------------------------------------------------------- |
| API can't connect to DB  | Check `DATABASE_URL` in `.env`; verify PostgreSQL is running: `docker compose ps floatctf-db`      |
| RustFS connection issues | Verify `RUSTFS_ADDRESS` matches the container hostname; check `RUSTFS_ENDPOINT_URL`                |
| SSL certificate errors   | Default uses self-signed certs; replace `app/keys/fullchain.pem` and `app/keys/privkey.pem` with trusted ones |

## License

This project is released under the [GNU AGPLv3](LICENSE) license. Copyright (C) 2025-2026 fb0sh@outlook.com