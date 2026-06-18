# Tasks — Cortex v2 Universal Service Gateway

> Derivado de: `01-requirements.md` (25 requisitos) y `02-design.md` (diseño de arquitectura)
> Stack: Rust, Axum, Tokio, async-trait, reqwest, governor, serde, sqlx (opcional)

---

## Fase 1: Fundación del Gateway Core

### 1.1 — Workspace y scaffolding

- [ ] T1.1.1 Crear Cargo workspace `cortex-v2/` con miembros `crates/cortex-core`, `crates/cortex-gateway`
- [ ] T1.1.2 Configurar `crates/cortex-core/Cargo.toml` con dependencias base: `tokio`, `async-trait`, `serde`, `serde_json`, `reqwest`, `futures`, `governor`, `jsonwebtoken`, `toml`, `tracing`, `tracing-subscriber`, `metrics`, `metrics-exporter-prometheus`, `thiserror`, `anyhow`, `dashmap`, `arc-swap`
- [ ] T1.1.3 Configurar `crates/cortex-gateway/Cargo.toml` con dependencias: `cortex-core`, `axum`, `tower`, `tower-http`, `hyper`, `uuid`
- [ ] T1.1.4 Crear `.gitignore` para Rust (target/, Cargo.lock, archivos de env)
- [ ] T1.1.5 Agregar `rust-toolchain.toml` fijando Rust 1.85+

### 1.2 — Trait Worker y entidades base

- [ ] T1.2.1 Definir `ServiceRequest`, `UserContext`, `OAuthToken` en `cortex-core/src/types.rs`
- [ ] T1.2.2 Definir `ServiceResponse`, `StreamChunk`, `UsageInfo` en `cortex-core/src/types.rs`
- [ ] T1.2.3 Definir `HealthStatus` enum en `cortex-core/src/worker.rs`
- [ ] T1.2.4 Definir `WorkerError` enum con variantes: `HttpError`, `RateLimited`, `AuthFailed`, `Timeout`, `ConfigError`, `ParseError`, `Other`
- [ ] T1.2.5 Definir el trait `Worker` con métodos: `name()`, `service_type()`, `capabilities()`, `priority()`, `execute()`, `execute_stream()` (default), `health_check()`, `api_keys()`, `rotate_api_key()`, `metadata()`
- [ ] T1.2.6 Definir `WorkerMetadata` struct
- [ ] T1.2.7 Tests unitarios del trait con un mock worker

### 1.3 — Configuración

- [ ] T1.3.1 Definir structs de configuración: `GatewayConfig`, `ThalamusConfig`, `RouterConfig`, `ObservabilityConfig`, `CacheConfig`, `DLQConfig`, `ConnectionPoolConfig`, `DedupConfig`
- [ ] T1.3.2 Definir `WorkerConfig` struct con todos los campos: `enabled`, `service_type`, `api_keys`, `default_model`, `timeout_secs`, `priority`, `capabilities`, `base_url`, `rate_limit_rpm`, `warmup`, `warmup_timeout_secs`
- [ ] T1.3.3 Implementar `Config::load()` — carga desde `cortex.toml` + override con env vars (`CORTEX_*`)
- [ ] T1.3.4 Implementar resolución de referencias a env vars en strings (`${VAR_NAME}`)
- [ ] T1.3.5 Tests unitarios: archivo válido, malformado, env var override, referencias anidadas

### 1.4 — Worker Registry

- [ ] T1.4.1 Implementar `WorkerRegistry` con `DashMap<String, Box<dyn Worker>>`
- [ ] T1.4.2 Métodos: `register()`, `unregister()`, `get()`, `list_by_service_type()`, `list_all()`
- [ ] T1.4.3 Implementar `WorkerRegistry::load_from_config()` — instancia workers desde `cortex.toml`
- [ ] T1.4.4 Manejar worker con `enabled = false` (no registrar, log warning)
- [ ] T1.4.5 Tests unitarios con mock workers

### 1.5 — Rate Limiter (GCRA)

- [ ] T1.5.1 Implementar `RateLimiter` usando la crate `governor`
- [ ] T1.5.2 Global rate limit (`global_rate_limit_rpm`)
- [ ] T1.5.3 Per-user rate limit extraído de claims JWT (`sub`)
- [ ] T1.5.4 Per-worker rate limit (`rate_limit_rpm` en worker config)
- [ ] T1.5.5 Método `check()` que devuelva `Result<(), RateLimitError>` con `retry-after`
- [ ] T1.5.6 Tests unitarios: rate limit alcanzado, recuperación, múltiples usuarios

### 1.6 — Gestión de credenciales y API keys

- [ ] T1.6.1 Implementar `CredentialManager` con estrategias de rotación: `RoundRobin`, `LeastUsed`, `Random`
- [ ] T1.6.2 Almacenar API keys con estado: `active`, `rate_limited_until`
- [ ] T1.6.3 Método `get_key()` — retorna la próxima key activa según estrategia
- [ ] T1.6.4 Método `mark_rate_limited(key)` — bloquea key por 15 minutos
- [ ] T1.6.5 Método `record_success(key)` — para estrategia `LeastUsed`
- [ ] T1.6.6 Si todas las keys están rate-limited, devolver error para que el router degrade al worker
- [ ] T1.6.7 Tests unitarios: rotación round-robin, bloqueo 429, todas agotadas

---

## Fase 2: HTTP Layer + Gateway Binary

### 2.1 — Servidor Axum

- [ ] T2.1.1 Crear `crates/cortex-gateway/src/main.rs` — entry point
- [ ] T2.1.2 Crear `crates/cortex-gateway/src/server.rs` — setup de Axum app con:
  - Router con todas las rutas
  - CORS configurado
  - Tower layers: `TraceLayer`, `RequestIdLayer`, `CorsLayer`
- [ ] T2.1.3 Graceful shutdown handler — escucha SIGTERM, drena requests, sale con 0

### 2.2 — Middleware de autenticación JWT

- [ ] T2.2.1 Implementar `cortex-core/src/auth.rs` — `JwtValidator`
- [ ] T2.2.2 Cliente HTTP para obtener JWKS de Thalamus (`GET /.well-known/jwks.json`)
- [ ] T2.2.3 Caché en memoria de JWKS con TTL (1h default) + jitter en refresh
- [ ] T2.2.4 Validación de JWT: firma, expiración, claims requeridos (`sub`, `org_id`, `scopes`)
- [ ] T2.2.5 Extraer `oauth_tokens` claim cuando esté presente
- [ ] T2.2.6 Axum middleware/extractor que inyecte `UserContext` en requests
- [ ] T2.2.7 Devolver 401 con `{"error": "unauthorized"}` para JWT inválido/expirado
- [ ] T2.2.8 Tests: JWT válido, expirado, firma inválida, Thalamus caído (cache), sin JWT

### 2.3 — Middleware de rate limiting

- [ ] T2.3.1 Axum middleware que llame a `RateLimiter::check()` antes de enrutar
- [ ] T2.3.2 Devolver 429 con header `Retry-After` y body `{"error": "rate_limited"}`
- [ ] T2.3.3 Aplicar rate limit global primero, luego por usuario

### 2.4 — Rutas de chat/completions

- [ ] T2.4.1 `POST /api/chat` — endpoint unificado de chat
- [ ] T2.4.2 `POST /v1/chat/completions` — endpoint compatible con OpenAI
- [ ] T2.4.3 Extraer `provider`, `model`, `stream`, `temperature`, `max_tokens`, `tools`, `messages` del body
- [ ] T2.4.4 Auto-detección de worker por `model` si no se especifica `provider`
- [ ] T2.4.5 Enrutar a `router.route()` o `router.route_stream()` según `stream`
- [ ] T2.4.6 Formatear respuesta no-streaming como OpenAI (`id`, `object`, `model`, `choices`, `usage`)
- [ ] T2.4.7 Emitir SSE para streaming: `data: {...}`, `event: done` al final
- [ ] T2.4.8 Incluir metadata en `done`: `model`, `worker`, `failover`, `usage`
- [ ] T2.4.9 Tests de integración con mock workers

### 2.5 — Rutas de búsqueda

- [ ] T2.5.1 `POST /api/search` — endpoint de búsqueda unificado
- [ ] T2.5.2 Extraer `query`, `provider`, `max_results` del body
- [ ] T2.5.3 Formatear resultados en formato unificado: `[{title, url, snippet, source}]`
- [ ] T2.5.4 Devolver 503 si no hay workers de búsqueda disponibles
- [ ] T2.5.5 Tests de integración

### 2.6 — Rutas de servicios genéricos

- [ ] T2.6.1 `POST /api/services/{service_type}` — endpoint genérico para business workers
- [ ] T2.6.2 Extraer `action` y `payload` del body
- [ ] T2.6.3 Enrutar a workers de `service_type` coincidente
- [ ] T2.6.4 Devolver 400 si no hay worker registrado para ese `service_type`
- [ ] T2.6.5 Tests de integración con mock business worker

### 2.7 — Rutas de health, models y stats

- [ ] T2.7.1 `GET /api/health` — estado global: `status`, `version`, `uptime_seconds`, `workers_total`, `workers_healthy`, `workers_degraded`
- [ ] T2.7.2 `GET /api/health/detailed` — estado por worker: `name`, `type`, `status`, `model`, `capabilities`, `requests_total`, `avg_latency_ms`
- [ ] T2.7.3 `GET /api/models` y `GET /v1/models` — listar modelos disponibles en formato OpenAI
- [ ] T2.7.4 `GET /api/services` — listar `service_type` registrados y workers disponibles
- [ ] T2.7.5 `GET /api/stats` — métricas agregadas: requests total/completados/fallidos, latencia promedio
- [ ] T2.7.6 `GET /metrics` — endpoint Prometheus

### 2.8 — Observabilidad

- [ ] T2.8.1 Implementar `Observability` en `cortex-core/src/observability.rs`
- [ ] T2.8.2 Métricas: contador de requests (por worker, status, usuario), histograma de latencia, tokens procesados (LLM)
- [ ] T2.8.3 OpenTelemetry: propagación de trace context a workers upstream, exportación al collector
- [ ] T2.8.4 Logs estructurados JSON via `tracing-subscriber` con niveles configurados
- [ ] T2.8.5 Enmascarar API keys en logs (`sk-...xxxx`)
- [ ] T2.8.6 Tests: verificar que métricas se emiten, que API keys no aparecen en logs

---

## Fase 3: Router con Patrones de Resiliencia

### 3.1 — Circuit Breaker + Failover

- [ ] T3.1.1 Implementar `CircuitBreaker` con estados: `Closed`, `Open`, `HalfOpen`
- [ ] T3.1.2 Transición `Closed → Open`: 3 fallos consecutivos
- [ ] T3.1.3 Transición `Open → HalfOpen`: después de `degradation_ttl_secs` (30s default)
- [ ] T3.1.4 Transición `HalfOpen → Closed`: 1 request exitoso
- [ ] T3.1.5 Transición `HalfOpen → Open`: 1 request fallido, reinicia cooldown
- [ ] T3.1.6 HTTP 429 saltea los 3 strikes → degradación inmediata
- [ ] T3.1.7 Tests unitarios: todas las transiciones de estado, 429 directo

### 3.2 — Jitter

- [ ] T3.2.1 Implementar `jittered_delay(base_ms, jitter_pct)` con `ChaCha8Rng`
- [ ] T3.2.2 Aplicar jitter a recovery de circuit breaker (cooldown + jitter)
- [ ] T3.2.3 Aplicar jitter a delays entre intentos de failover (20-200ms)
- [ ] T3.2.4 Aplicar jitter a refresh de JWKS de Thalamus
- [ ] T3.2.5 Tests: verificar que delays se distribuyen dentro de ±25%

### 3.3 — Retry con backoff exponencial

- [ ] T3.3.1 Implementar lógica de reintentos: hasta 3 intentos, backoff 1s→2s→4s
- [ ] T3.3.2 Aplicar jitter a cada delay de reintento
- [ ] T3.3.3 Solo reintentar en errores 5xx o de red (no reintentar 4xx excepto 429)
- [ ] T3.3.4 Cancelar reintentos si se excede `request_timeout_secs`
- [ ] T3.3.5 Tests: reintentos exitosos, reintentos agotados, timeout durante reintentos

### 3.4 — Bulkhead (límite de concurrencia)

- [ ] T3.4.1 Implementar semáforo global (`Arc<Semaphore>`) con `max_concurrent_requests`
- [ ] T3.4.2 Implementar semáforo por worker con `max_concurrent_per_worker`
- [ ] T3.4.3 En `Router::route()`: adquirir semáforo global first, luego per-worker
- [ ] T3.4.4 Si semáforo global lleno: devolver `RouterError::Overloaded` → HTTP 503
- [ ] T3.4.5 Si semáforo de worker lleno: saltar al siguiente worker del mismo service_type
- [ ] T3.4.6 Liberar slot de concurrencia al finalizar (éxito o error)
- [ ] T3.4.7 Tests: rechazo por bulkhead global, skip de worker saturado

### 3.5 — Backpressure queue

- [ ] T3.5.1 Implementar cola FIFO acotada con `tokio::sync::mpsc` de tamaño `backpressure_queue_size`
- [ ] T3.5.2 Cuando semáforo global está lleno y backpressure habilitado: encolar en vez de rechazar
- [ ] T3.5.3 Si cola llena: devolver 503 inmediato
- [ ] T3.5.4 Timeout en cola: si un request pasa > `request_timeout_secs` encolado → 503
- [ ] T3.5.5 Desencolar el request más antiguo cuando se libera un slot de concurrencia
- [ ] T3.5.6 Configurar `backpressure_enabled = true/false` desde config
- [ ] T3.5.7 Tests: encolamiento exitoso, cola llena, timeout en cola

### 3.6 — Idempotency keys

- [ ] T3.6.1 Implementar caché LRU de idempotencia: `(key, response, timestamp)`
- [ ] T3.6.2 Al recibir request con `Idempotency-Key`: buscar en caché
- [ ] T3.6.3 Si encontrada y no expirada: devolver respuesta cacheada
- [ ] T3.6.4 Si no encontrada: ejecutar request, guardar respuesta en caché por `idempotency_ttl_secs`
- [ ] T3.6.5 Si request con misma key está en vuelo: esperar con `oneshot` channel
- [ ] T3.6.6 Para workers con `side_effects = true`: requerir `Idempotency-Key` en acciones mutantes
- [ ] T3.6.7 Si falta `Idempotency-Key` en worker con side_effects: 400 con mensaje de error
- [ ] T3.6.8 Tests: dedup exitoso, request en vuelo, key expirada, falta key requerida

### 3.7 — Response cache

- [ ] T3.7.1 Implementar caché de respuestas con crate `moka` (sync o future)
- [ ] T3.7.2 Configurar TTL global por worker y por acción
- [ ] T3.7.3 `Worker::cacheable_actions()` — trait method que declara acciones cacheables
- [ ] T3.7.4 En `Router::route()`: antes de ejecutar, consultar caché si la acción es cacheable
- [ ] T3.7.5 Si cache hit y TTL válido: devolver respuesta cacheada
- [ ] T3.7.6 Si cache miss: ejecutar, guardar en caché con TTL
- [ ] T3.7.7 Nunca cachear acciones con side effects (`crear_orden_compra`, etc.)
- [ ] T3.7.8 Evicción LRU cuando se alcanza `max_entries`
- [ ] T3.7.9 Tests: cache hit, miss, TTL expirado, acción no cacheable

### 3.8 — Dead Letter Queue

- [ ] T3.8.1 Implementar `DeadLetterQueue` con storage SQLite (usando `rusqlite`)
- [ ] T3.8.2 Schema: `id`, `worker_name`, `service_type`, `action`, `payload`, `user_sub`, `retry_count`, `max_retries`, `next_retry_at`, `status`, `created_at`, `updated_at`
- [ ] T3.8.3 En `Router::route()`: si todos los workers de un service_type con `side_effects = true` fallan → persistir en DLQ
- [ ] T3.8.4 Background worker: cada 60s, procesar entradas pendientes con `next_retry_at <= now()`
- [ ] T3.8.5 Reintentos con backoff: 1m→2m→4m→8m→16m hasta `max_retries`
- [ ] T3.8.6 Si se agotan reintentos: marcar como `failed_permanent`
- [ ] T3.8.7 Tests: entrada en DLQ, reintento exitoso, reintentos agotados

### 3.9 — Warm-up

- [ ] T3.9.1 Al iniciar, identificar workers con `warmup = true`
- [ ] T3.9.2 Ejecutar `worker.health_check()` para cada uno concurrentemente
- [ ] T3.9.3 Mientras warm-up en progreso: `/api/health` devuelve `status: "starting"` con HTTP 503
- [ ] T3.9.4 Si todos pasan o `warmup_timeout_secs` expira: transicionar a estado real
- [ ] T3.9.5 Workers que fallan warm-up: marcar como `degraded`, gateway arranca en `degraded`
- [ ] T3.9.6 Tests: warm-up exitoso, timeout, worker falla

### 3.10 — Deduplicación de requests

- [ ] T3.10.1 Implementar `RequestDeduplicator` con ventana temporal configurable (`window_ms`)
- [ ] T3.10.2 Clave de dedup: `(user_sub, service_type, action, hash(payload))`
- [ ] T3.10.3 Si request duplicado detectado dentro de la ventana:
  - Si original en vuelo: esperar y devolver misma respuesta
  - Si original completado: devolver respuesta cacheada
- [ ] T3.10.4 Si `Idempotency-Key` presente: el mecanismo de idempotencia tiene precedencia
- [ ] T3.10.5 Tests: duplicado detectado, fuera de ventana, precedencia de idempotency key

### 3.11 — Graceful shutdown

- [ ] T3.11.1 Escuchar SIGTERM/SIGINT con `tokio::signal`
- [ ] T3.11.2 Al recibir señal: health check cambia a 503
- [ ] T3.11.3 Dejar de aceptar nuevas conexiones TCP
- [ ] T3.11.4 Esperar `shutdown_timeout_secs` para drenar requests en vuelo
- [ ] T3.11.5 Cancelar requests que exceden el timeout, emitir `event: error` para streaming
- [ ] T3.11.6 Flush de entradas pendientes en audit log y DLQ
- [ ] T3.11.7 Deregister del service mesh de ZEA (si configurado)
- [ ] T3.11.8 Exit(0)
- [ ] T3.11.9 Tests: shutdown con requests en vuelo, timeout, streaming cancelado

### 3.12 — Connection pool

- [ ] T3.12.1 Configurar `reqwest::Client` con pool limits: `max_idle_per_host`, `pool_max_idle_timeout`, `pool_max_size`, `tcp_keepalive`, `connect_timeout`
- [ ] T3.12.2 Permitir overrides por worker en configuración TOML
- [ ] T3.12.3 Compartir una sola instancia de `reqwest::Client` entre todos los workers
- [ ] T3.12.4 Tests: verificar que el pool reutiliza conexiones

---

## Fase 4: Workers — Implementación por Crate

### 4.1 — Worker DeepSeek

- [ ] T4.1.1 Crear crate `crates/workers/deepseek/` con `Cargo.toml` propio
- [ ] T4.1.2 Implementar `Worker` trait para `DeepSeekWorker`
- [ ] T4.1.3 `execute()`: POST a `/chat/completions`, formato DeepSeek API
- [ ] T4.1.4 `execute_stream()`: SSE streaming desde DeepSeek API
- [ ] T4.1.5 Parseo de respuesta: extraer `choices[0].message.content`, `usage`
- [ ] T4.1.6 `health_check()`: GET a endpoint de health de DeepSeek (o request mínimo)
- [ ] T4.1.7 Mapear `WorkerError::RateLimited` en HTTP 429
- [ ] T4.1.8 Configuración: `api_keys`, `base_url`, `default_model`, `timeout_secs`
- [ ] T4.1.9 Tests de integración con `wiremock`

### 4.2 — Worker Anthropic

- [ ] T4.2.1 Crear crate `crates/workers/anthropic/`
- [ ] T4.2.2 Implementar `Worker` trait para `AnthropicWorker`
- [ ] T4.2.3 `execute()`: POST a `/v1/messages`, formato Anthropic Messages API
- [ ] T4.2.4 `execute_stream()`: SSE streaming desde Anthropic
- [ ] T4.2.5 Mapear mensajes de formato OpenAI a Anthropic (`system`, `messages`)
- [ ] T4.2.6 `health_check()`
- [ ] T4.2.7 Tests de integración con `wiremock`

### 4.3 — Worker OpenAI

- [ ] T4.3.1 Crear crate `crates/workers/openai/`
- [ ] T4.3.2 Implementar `Worker` trait para `OpenAIWorker`
- [ ] T4.3.3 `execute()`: POST a `/v1/chat/completions`, formato OpenAI nativo
- [ ] T4.3.4 `execute_stream()`: SSE streaming desde OpenAI
- [ ] T4.3.5 `health_check()`
- [ ] T4.3.6 Tests de integración con `wiremock`

### 4.4 — Worker Gemini

- [ ] T4.4.1 Crear crate `crates/workers/gemini/`
- [ ] T4.4.2 Implementar `Worker` trait para `GeminiWorker`
- [ ] T4.4.3 `execute()`: POST a `/v1beta/models/{model}:generateContent`
- [ ] T4.4.4 `execute_stream()`: SSE streaming desde Gemini
- [ ] T4.4.5 Mapear messages OpenAI → formato Gemini (`contents`, `parts`)
- [ ] T4.4.6 `health_check()`
- [ ] T4.4.7 Tests de integración con `wiremock`

### 4.5 — Worker Groq

- [ ] T4.5.1 Crear crate `crates/workers/groq/`
- [ ] T4.5.2 Implementar `Worker` trait para `GroqWorker`
- [ ] T4.5.3 `execute()`: POST a `/openai/v1/chat/completions` (Groq es OpenAI-compatible)
- [ ] T4.5.4 `execute_stream()`: SSE streaming desde Groq
- [ ] T4.5.5 `health_check()`
- [ ] T4.5.6 Tests de integración con `wiremock`

### 4.6 — Worker Ollama

- [ ] T4.6.1 Crear crate `crates/workers/ollama/`
- [ ] T4.6.2 Implementar `Worker` trait para `OllamaWorker`
- [ ] T4.6.3 `execute()`: POST a `/api/chat` (Ollama API local)
- [ ] T4.6.4 `execute_stream()`: NDJSON streaming desde Ollama
- [ ] T4.6.5 `health_check()`: GET `/api/tags`
- [ ] T4.6.6 No requiere API key
- [ ] T4.6.7 Tests de integración con `wiremock`

### 4.7 — Worker Brave

- [ ] T4.7.1 Crear crate `crates/workers/brave/`
- [ ] T4.7.2 Implementar `Worker` trait para `BraveWorker`
- [ ] T4.7.3 `execute()`: GET a Brave Search API
- [ ] T4.7.4 Parsear resultados web a formato unificado `[{title, url, snippet}]`
- [ ] T4.7.5 `health_check()`
- [ ] T4.7.6 Tests de integración

### 4.8 — Worker Serper

- [ ] T4.8.1 Crear crate `crates/workers/serper/`
- [ ] T4.8.2 Implementar `Worker` trait para `SerperWorker`
- [ ] T4.8.3 `execute()`: POST a Serper API
- [ ] T4.8.4 Parsear resultados a formato unificado
- [ ] T4.8.5 Tests de integración

### 4.9 — Worker Tavily

- [ ] T4.9.1 Crear crate `crates/workers/tavily/`
- [ ] T4.9.2 Implementar `Worker` trait para `TavilyWorker`
- [ ] T4.9.3 `execute()`: POST a Tavily Search API
- [ ] T4.9.4 Tests de integración

### 4.10 — Worker DuckDuckGo

- [ ] T4.10.1 Crear crate `crates/workers/duckduckgo/`
- [ ] T4.10.2 Implementar `Worker` trait para `DuckDuckGoWorker`
- [ ] T4.10.3 `execute()`: GET a DuckDuckGo Instant Answer API (sin API key)
- [ ] T4.10.4 Tests de integración

### 4.11 — Worker PubMed

- [ ] T4.11.1 Crear crate `crates/workers/pubmed/`
- [ ] T4.11.2 Implementar `Worker` trait para `PubMedWorker`
- [ ] T4.11.3 `execute()`: consultar Entrez API, soportar `max_results` y `sort`
- [ ] T4.11.4 Tests de integración

### 4.12 — Worker SII Chile

- [ ] T4.12.1 Crear crate `crates/workers/sii/` con estructura `lib.rs`, `client.rs`, `parser.rs`
- [ ] T4.12.2 Implementar `Worker` trait para `SIIWorker`
- [ ] T4.12.3 `execute()`: acción `consultar_rut` → scrape/parse HTML de SII → JSON
- [ ] T4.12.4 `execute()`: acción `consultar_estado_tributario`
- [ ] T4.12.5 `parser.rs`: extraer `razon_social`, `actividades`, `tramo_ventas`, `fecha_inicio` del HTML
- [ ] T4.12.6 `health_check()`: request mínimo al SII
- [ ] T4.12.7 Manejar rate limit estricto del SII (60 RPM)
- [ ] T4.12.8 Marcar `side_effects = true` para facturación electrónica
- [ ] T4.12.9 Marcar acciones de lectura como `cacheable_actions()`
- [ ] T4.12.10 Tests de integración con HTML simulado del SII

### 4.13 — Worker iConstruye

- [ ] T4.13.1 Crear crate `crates/workers/iconstruye/` con `lib.rs`, `client.rs`, `auth.rs`
- [ ] T4.13.2 Implementar `Worker` trait para `IConstruyeWorker`
- [ ] T4.13.3 `auth.rs`: manejo de sesión — detectar 401, refrescar token, reintentar
- [ ] T4.13.4 `execute()`: acción `buscar_proveedores` → GET/POST a API iConstruye
- [ ] T4.13.5 `execute()`: acción `crear_orden_compra` → POST a API iConstruye
- [ ] T4.13.6 `execute()`: acción `consultar_cotizacion` y `listar_proyectos`
- [ ] T4.13.7 `health_check()`: request mínimo a iConstruye
- [ ] T4.13.8 Marcar `side_effects = true` para acciones mutantes
- [ ] T4.13.9 Tests de integración con `wiremock`

---

## Fase 5: CLI de Administración

### 5.1 — CLI binary

- [ ] T5.1.1 Crear crate `crates/cortex-cli/` con `Cargo.toml` propio
- [ ] T5.1.2 Usar `clap` para parsing de argumentos y subcomandos
- [ ] T5.1.3 Conectarse al gateway via HTTP (`CORTEX_GATEWAY_URL` env var, default `http://localhost:4000`)

### 5.2 — Comandos

- [ ] T5.2.1 `cortex status` — GET `/api/health` y mostrar versión, uptime, resumen de workers
- [ ] T5.2.2 `cortex workers list` — GET `/api/services` y mostrar tabla de workers
- [ ] T5.2.3 `cortex workers show <name>` — GET `/api/health/detailed` y filtrar por worker
- [ ] T5.2.4 `cortex test chat --provider <p> <message>` — POST `/api/chat` y hacer stream a stdout
- [ ] T5.2.5 `cortex test search <query>` — POST `/api/search` y mostrar resultados
- [ ] T5.2.6 `cortex config reload` — POST `/api/admin/config/reload` para recarga en caliente
- [ ] T5.2.7 `cortex dlq list` — listar entradas pendientes/failures en DLQ
- [ ] T5.2.8 `cortex dlq show <id>` — ver detalle de entrada DLQ
- [ ] T5.2.9 `cortex dlq retry <id>` — reintentar entrada DLQ
- [ ] T5.2.10 `cortex dlq discard <id>` — descartar entrada DLQ
- [ ] T5.2.11 `cortex dlq purge` — limpiar entradas procesadas
- [ ] T5.2.12 `cortex stats` — GET `/api/stats` y mostrar tabla de métricas

---

## Fase 6: Testing, Integración y E2E

### 6.1 — Test infrastructure

- [ ] T6.1.1 Crear `MockWorker` reutilizable para tests del core
- [ ] T6.1.2 Configurar `wiremock` en workspace `[dev-dependencies]`
- [ ] T6.1.3 Agregar `cargo test --workspace` al CI

### 6.2 — Integration tests por worker

- [ ] T6.2.1 Test DeepSeek: chat simple, streaming, error 429, timeout
- [ ] T6.2.2 Test Anthropic: chat simple, streaming, error handling
- [ ] T6.2.3 Test OpenAI: chat simple, streaming, tools
- [ ] T6.2.4 Test Gemini: chat simple, streaming
- [ ] T6.2.5 Test Groq: chat simple, streaming
- [ ] T6.2.6 Test Ollama: chat simple, streaming
- [ ] T6.2.7 Test Brave: búsqueda, rate limit
- [ ] T6.2.8 Test Serper: búsqueda
- [ ] T6.2.9 Test Tavily: búsqueda
- [ ] T6.2.10 Test DuckDuckGo: búsqueda
- [ ] T6.2.11 Test PubMed: búsqueda académica con parámetros
- [ ] T6.2.12 Test SII: consultar_rut con HTML parse, estado tributario
- [ ] T6.2.13 Test iConstruye: buscar_proveedores, crear_orden_compra, refresh de auth

### 6.3 — Integration tests del router

- [ ] T6.3.1 Test failover: worker 1 falla, worker 2 responde → failover exitoso
- [ ] T6.3.2 Test circuit breaker: 3 fallos → breaker abre → half-open → cierra
- [ ] T6.3.3 Test circuit breaker con proveedor explícito: breaker abierto → error inmediato
- [ ] T6.3.4 Test bulkhead global: requests > max_concurrent → 503
- [ ] T6.3.5 Test bulkhead per-worker: worker saturado → skip al siguiente
- [ ] T6.3.6 Test backpressure: cola funciona, timeout en cola
- [ ] T6.3.7 Test idempotency: key duplicada devuelve respuesta cacheada
- [ ] T6.3.8 Test idempotency: request en vuelo → esperar
- [ ] T6.3.9 Test response cache: cache hit, TTL expirado
- [ ] T6.3.10 Test DLQ: request falla todos los workers → entra en DLQ → reintento automático
- [ ] T6.3.11 Test warm-up: gateway devuelve 503 hasta que workers listos
- [ ] T6.3.12 Test dedup: requests idénticos en 300ms → segundo recibe misma respuesta
- [ ] T6.3.13 Test graceful shutdown: requests en vuelo se completan antes de exit
- [ ] T6.3.14 Test JWT: válido, expirado, sin JWT, Thalamus caído

### 6.4 — E2E tests del gateway binary

- [ ] T6.4.1 Iniciar gateway en test, enviar request de chat real → verificar respuesta
- [ ] T6.4.2 Iniciar gateway en test, enviar request de búsqueda → verificar respuesta
- [ ] T6.4.3 Iniciar gateway en test, enviar request de servicio de negocio → verificar
- [ ] T6.4.4 Verificar `/api/health` y `/metrics` tras requests
- [ ] T6.4.5 Verificar formato OpenAI en `/v1/chat/completions` y `/v1/models`

### 6.5 — Performance tests

- [ ] T6.5.1 Script de carga con `k6` o `drill`: 1000 requests concurrentes
- [ ] T6.5.2 Medir overhead del gateway (< 5ms P95 sobre latencia del worker)
- [ ] T6.5.3 Medir memoria en reposo (< 50MB)
- [ ] T6.5.4 Medir throughput con streaming (100 conexiones SSE simultáneas)

---

## Fase 7: Despliegue y Documentación

### 7.1 — Docker

- [ ] T7.1.1 Crear `Dockerfile` multi-stage con `rust:alpine` → `scratch`
- [ ] T7.1.2 Compilar con target `x86_64-unknown-linux-musl`
- [ ] T7.1.3 HEALTHCHECK con `/api/health`
- [ ] T7.1.4 `docker-compose.yml` con Cortex v2 + Thalamus + PostgreSQL (opcional)
- [ ] T7.1.5 Verificar tamaño de imagen < 20MB

### 7.2 — CI/CD

- [ ] T7.2.1 GitHub Actions: `cargo test --workspace`
- [ ] T7.2.2 GitHub Actions: `cargo clippy -- -D warnings`
- [ ] T7.2.3 GitHub Actions: `cargo fmt --check`
- [ ] T7.2.4 GitHub Actions: build release binary + Docker image

### 7.3 — Documentación

- [ ] T7.3.1 `README.md` con: descripción, quickstart, configuración, arquitectura
- [ ] T7.3.2 Guía de workers: cómo implementar un worker nuevo paso a paso
- [ ] T7.3.3 Documentación del trait `Worker` (`cargo doc`)
- [ ] T7.3.4 Ejemplos de `cortex.toml` comentados para distintos escenarios
- [ ] T7.3.5 Guía de integración con Thalamus

---

## Resumen de fases

| Fase | Descripción | Tareas | Depende de |
|------|-------------|--------|------------|
| 1 | Fundación del Gateway Core | ~22 | — |
| 2 | HTTP Layer + Gateway Binary | ~20 | Fase 1 |
| 3 | Router con Patrones de Resiliencia | ~40 | Fase 1, Fase 2 |
| 4 | Workers (13 crates) | ~50 | Fase 1 |
| 5 | CLI de Administración | ~12 | Fase 2 |
| 6 | Testing, Integración y E2E | ~30 | Fases 1-5 |
| 7 | Despliegue y Documentación | ~10 | Fases 1-6 |

**Total estimado: ~184 tareas**
