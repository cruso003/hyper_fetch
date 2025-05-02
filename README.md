# Hyper Fetch API

A custom web scraper API built to replace external APIs like JSearch, providing endpoints for YouTube videos and remote job listings.

## Overview

This project is a Rust-based API using Actix Web to scrape data from YouTube and RemoteOK. It includes caching, rate limiting, and Swagger documentation for easy integration.

## Features

- **YouTube Scraper**: Fetch video tutorials based on a query.
- **Job Scraper**: Fetch remote job listings from RemoteOK with filters for location, job type, and remote-only.
- **Caching**: Results are cached for 4 hours to reduce load on external sites.
- **Rate Limiting**: 100 requests per minute per IP to prevent abuse.
- **Swagger UI**: API documentation available at `/swagger-ui/`.

## Prerequisites

- Rust (stable, latest version recommended)
- Cargo
- Git

## Setup

1. **Clone the Repository**:

   ```bash
   git clone <repository-url>
   cd hyper-fetch-api
   ```

2. **Install Dependencies**:

   ```bash
   cargo build
   ```

3. **Run the Server**:

   ```bash
   cargo run
   ```

   The server will start on `http://127.0.0.1:8081`.

## API Endpoints

- **Health Check**: `GET /api/v1/health`
  - Returns: `"Healthy"`
- **Echo**: `GET /api/v1/echo`
  - Returns: `"Hello, world!"`
- **Get YouTube Videos**: `GET /api/v1/resources/video`
  - Query Parameters:
    - `query` (required): Search term (e.g., "rust tutorial").
    - `limit` (optional): Number of videos (default: 5).
    - `sorting` (optional): Sorting method (default: "relevance").
  - Example: `curl "http://127.0.0.1:8081/api/v1/resources/video?query=rust+tutorial&limit=3"`
- **Get Jobs**: `GET /api/v1/jobs`
  - Query Parameters:
    - `query` (required): Job search term (e.g., "software engineer").
    - `limit` (optional): Number of jobs (default: 10).
    - `location` (optional): Location filter (e.g., "San Francisco").
    - `remote_only` (optional): Filter for remote jobs (true/false).
    - `job_type` (optional): Job type filter (e.g., "Full-time", "Contract").
  - Example: `curl "http://127.0.0.1:8081/api/v1/jobs?query=software+engineer&limit=5&remote_only=true"`
- **Clear Cache**: `GET /api/v1/cache/clear`
  - Clears all cached data.
- **Refresh Cache**: `GET /api/v1/cache/refresh`
  - Query Parameters:
    - `cache_key` (required): Key to refresh.
  - Example: `curl "http://127.0.0.1:8081/api/v1/cache/refresh?cache_key=jobs_software_engineer_10__true_"`

## Swagger Documentation

Access the Swagger UI at `http://127.0.0.1:8081/swagger-ui/` to explore the API interactively.

## Rate Limiting

The API enforces a rate limit of 100 requests per minute per IP. If exceeded, you’ll receive a `429 Too Many Requests` response.

## Development

- **Dependencies**: Managed via `Cargo.toml`.
- **Logging**: Uses `env_logger` with the `info` level by default.
- **Caching**: Implemented in `cache.rs` with a 4-hour TTL.

## Publishing

To publish as a replacement for an external API:

1. Deploy to a server (e.g., AWS, Heroku, or a VPS).
2. Update your application to point to this API’s base URL.
3. Monitor logs for rate limiting or scraping issues.
4. Consider adding authentication if the API will be public.

## License

MIT License. See `LICENSE` for details.
