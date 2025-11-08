# Token Agent  
### Configurable Token Fetching & Propagation Service

Token Agent is a config-driven lightweight and flexible service for securely **fetching**, **caching**, and **propagating** tokens from multiple sources (HTTP, metadata APIs, files) to multiple destinations (HTTP endpoints, files, Unix sockets).

Supports chaining, transformation, and custom response formats — making it suitable for cloud metadata integration, token exchange, and service identity propagation in distributed systems.

Focus: reliability, small footprint, configurable via a single YAML contract, and predictable token rotation.

### Feature Highlights

✅ Config-driven identity flows (sources → sinks)

⚙️ Built-in retry, caching/invalidation, and metrics

🧠 Cross-cloud support (Metadata, STS, OAuth2, Managed Identity)

🔐 Secure, minimal Rust binary (systemd-ready)


### Why Token Agent?

Token Agent reads one config, builds the full token propagation graph, and keeps everything fresh.


---

## Table of Contents

- [Overview](#overview)
- [Quick Start Example](#quick-start-example)
- [Concepts](#concepts)
  - [Sources](#sources)
  - [Sinks](#sinks)
  - [Chaining & Dependencies](#chaining--dependencies)
- [Configuration Reference](#configuration-reference)
  - [Source Configuration](#source-configuration)
  - [Sink Configuration](#sink-configuration)
  - [Expiration Handling](#expiration-handling)
  - [Templating & Interpolation](#templating--interpolation)
- [Validation Rules](#validation-rules)
- [Installation](#installation)

---

## Overview

**Token Agent** retrieves tokens from one or more defined **sources**, optionally transforms or chains them, and exposes them through **sinks**.

The configuration is declarative and expressed in YAML, defining how each token is retrieved, parsed, and propagated.

Flow:

```
[Source A: Metadata API] 
     ↓
[Source B: Token Exchange] 
     ↓
[Sink: HTTP / File / UDS]
```

The service is platform-agnostic and designed primarily for **Linux environments** (runs as a systemd service, emits structured logs compatible with `journalctl` / `vector`), but can operate anywhere with a POSIX runtime.

---

## Quick Start Example

A minimal configuration that retrieves a metadata token, exchanges it for another service token, and exposes both via HTTP and files:

<details>
<summary>Show example</summary>

```yaml
<skipped settings block>

sources:
  metadata:
    type: http
    request:
      url: "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token"
      method: GET
      headers:
        "Metadata-Flavor":
          value: "Google"
    parse:
      tokens:
        - id: metadata_token
          parent: body
          pointer: "access_token"
          token_type: plain_text
          expiration:
            source: json_body_field
            pointer: "expires_in"
            format: seconds

  exchange_token_service:
    type: http
    inputs: ["metadata"]
    request:
      url: "https://exchange_token_service.internal.local/issueToken"
      method: POST
      headers:
        Authorization:
          template: "Bearer {{metadata.metadata_token}}"
          required: true
        "Content-Type":
          value: "application/json"
      body:
        subject_token:
          source: metadata
          id: metadata_token
        subject_token_type:
          value: "urn:ietf:params:oauth:token-type:access_token"
    parse:
      tokens:
        - id: exchange_token_service_token
          parent: body
          pointer: "access_token"
          token_type: jwt

sinks:
  metadata_file:
    type: file
    source_id: metadata
    token_id: metadata_token
    path: "/tmp/metadata.token"

  exchange_http:
    type: http
    source_id: exchange_token_service
    path: "/tokens/exchange_token_service"
    token_id: exchange_token_service_token
    response:
      content_type: "application/json"
      headers:
        X-Token:
          type: token
      body:
        access_token:
          type: token
        expires_in:
          type: expiration
          format: seconds
```

</details>

---

## Concepts

### Sources
A **source** defines how to fetch a token.  
Supported `type` values:
- `http` — fetch token via REST API (supports headers, body (JSON) parsing)
- `file` — read token from filesystem

Each source:
- Must have a unique ID (the key name)
- Can depend on other sources (chaining via `inputs`)

### Sinks
A **sink** defines how a token is exposed or stored.  
Supported `type` values:
- `file` — writes tokens to local files
- `http` — serves tokens via HTTP endpoints
- `uds` — exposes tokens via Unix domain sockets

### Chaining & Dependencies
Chaining allows one source to depend on another, e.g.:
```
inputs: ["metadata"]
```
This means the current source uses tokens retrieved from `metadata` to perform its request.

---

## Configuration Reference

### Source Configuration

#### Common Fields
| Field | Type | Description |
|-------|------|-------------|
| `type` | string | One of `http`, `file` |
| `inputs` | list | Optional. Defines dependency sources for chained token exchange. |

#### HTTP Source

##### `request` Block
Defines how to perform the token request.

| Field | Description |
|-------|-------------|
| `url` | Token endpoint URL |
| `method` | HTTP method (`GET` or `POST`) |
| `headers` | Map of custom headers |
| `body` | Optional JSON body fields |

Header value sources:

| Mode | Description | Result |
|------|--------------|----------|
| `value` | Static literal value | `"Metadata-Flavor": "Google"` |
```yaml
headers:
  Authorization:
    "Metadata-Flavor":
      value: "Google" # constant value
```
| `template` | Interpolated from another source | `"Authorization": "Bearer eykmdvlkmvd"` |
```yaml
headers:
  Authorization:
    template: "Bearer {{source_id.token_id}}" # token will be taken from source_id -> token_id 
    required: bool    #  if true → fail-fast if template cannot be rendered
```
| `source` | Direct reference from another source | `"Authorization": "Bearer eykmdvlkmvd"` |
```yaml
headers:
  Authorization:
    source: source_id 
    id: token_id        # token will be taken from source_id -> token_id 
    prefix: "Bearer "   # optional prefix for the token , f.e. "Bearer "
```
| `from_file` | Value read from file | `"Authorization": "Bearer eykmdvlkmvd"` |
```yaml
headers:
  Authorization:
    from_file: "/path/to/token"
    prefix: "Bearer "   # optional prefix for the token , f.e. "Bearer " // TODO
```
| `from_env` | Value read from environment | `"Authorization": "Bearer eykmdvlkmvd"` |
```yaml
headers:
  Authorization:
    from_env: "TOKEN_ENV"
    prefix: "Bearer "   # optional prefix for the token , f.e. "Bearer " // TODO
```

##### `parse` Block
Defines how to extract tokens from responses.

Each token definition:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Token ID (unique per source) |
| `parent` | string | `body` or `header` |
| `pointer` | string | JSON pointer or header key |
| `token_type` | string | `jwt` or `plain_text` |
| `expiration` | object | Expiration definition |

Expiration subfields:

| Field | Description |
|-------|-------------|
| `source` | One of `json_body_field`, `header_field`, `manual`, `self` |
| `pointer` | Required for field-based sources |
| `format` | One of `seconds`, `unix`, `rfc3339` |
| `manual_ttl_seconds` | Used if `source: manual` |

---

### Sink Configuration

#### Common Fields
| Field | Type | Description |
|-------|------|-------------|
| `type` | string | `http`, `file`, or `uds` |
| `input` | string | Source ID providing token |
| `token` | string | Token ID to use |

#### HTTP Sink

Serves tokens dynamically via HTTP.

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | URL path exposed via shared HTTP server |
| `response` | object | Response definition (headers + body) |

Response structure:

| Section | Description |
|----------|-------------|
| `content_type` | MIME type (e.g. `application/json`) |
| `headers` | Key/value pairs (can use token/string/expiration fields) |
| `body` | Key/value pairs (same structure as headers) |

Field types:
- `token` — returns a token value  
- `string` — static text  
- `expiration` — expiration info formatted as `seconds`, `rfc3339`, or `unix`

#### File Sink

Writes a token to a file.

| Field | Description |
|-------|-------------|
| `path` | Output path (e.g. `/tmp/token`) |
| `input` | Source providing the token |
| `token` | Token ID to write |

File sinks are **active** — tokens are written when updated and removed on invalidation.

---

## Expiration Handling

Supported formats:
| Format | Description |
|---------|-------------|
| `seconds` | Relative time until expiration |
| `unix` | UNIX timestamp (epoch seconds) |
| `rfc3339` | Absolute datetime (ISO-8601) |
| `manual` | Fixed lifetime in seconds |

Expired tokens are automatically invalidated in the cache.

---

## Templating & Interpolation

Token Agent supports Go-style variable interpolation for chaining:

```yaml
headers:
  Authorization:
    template: "Bearer {{metadata.metadata_token}}"
    required: bool
```

At runtime, `{{metadata.metadata_token}}` is replaced by the actual token value fetched from the `metadata` source.

If a `template` field has `required: true`, the agent will **fail fast** if rendering fails.

---

## Validation Rules

- Each source and sink ID must be **unique**.
- Each token ID must be **unique per source**.
- Chained dependencies (`inputs`) must be valid and resolvable.
- Paths for `file` sinks must be writable.
- All `template` variables must resolve at runtime if marked `required: true`.

## Installation

### ubuntu x86_64
```bash
wget https://github.com/AleksandrNi/token-agent/releases/download/v0.1.0/token-agent_0.1.0-1_amd64.deb
sudo dpkg -i token-agent_0.1.0-1_amd64.deb
sudo systemctl enable --now token-agent
```

### ubuntu ARM64
```bash
wget https://github.com/AleksandrNi/token-agent/releases/download/v0.1.0/token-agent_0.1.0-1_arm64.deb
sudo dpkg -i token-agent_0.1.0-1_arm64.deb
```

### macos intell
```bash
curl -L -O https://github.com/AleksandrNi/token-agent/releases/download/v0.1.0/token-agent_v0.1.0_macos_x86_64.tar.gz
tar -xzf token-agent_v0.1.0_macos_x86_64.tar.gz
./token-agent --help
```

### macos apple silicon
```bash
curl -L -O https://github.com/AleksandrNi/token-agent/releases/download/v0.1.0/token-agent_v0.1.0_macos_arm64.tar.gz
tar -xzf token-agent_v0.1.0_macos_arm64.tar.gz
./token-agent --help
```


### License
This project is licensed under the [MIT license](/LICENSE).

### Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `token-agent` by you, shall be licensed as MIT, without any
additional terms or conditions.
