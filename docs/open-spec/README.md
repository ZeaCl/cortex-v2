# Cortex v2 — Universal Service Gateway (Rust)

Especificación completa del gateway universal de servicios de ZEA Platform, reescrito desde cero en Rust.

## Documentos

| Fase | Archivo | Estado |
|------|---------|--------|
| 1 — Requisitos | [`01-requirements.md`](./01-requirements.md) | ✅ Completo (25 requisitos EARS) |
| 2 — Diseño | [`02-design.md`](./02-design.md) | ✅ Completo |
| 2b — Resiliencia | [`02b-resilience-guide.md`](./02b-resilience-guide.md) | ✅ Completo |
| 3 — Tareas | [`03-tasks.md`](./03-tasks.md) | ✅ Completo (~184 tareas en 7 fases) |

## Resumen

Cortex v2 evoluciona de un gateway de LLMs a un **enrutador universal de servicios**: cualquier API externa o interna se integra implementando el trait `Worker` y registrándose en el gateway. El core no sabe qué hace cada worker — solo rutea, aplica failover, rate limiting y telemetría.

### Stack

- **Lenguaje:** Rust 1.85+
- **HTTP:** Axum + Tokio
- **Cliente HTTP:** reqwest (conexiones keep-alive, streaming)
- **Rate limiting:** GCRA vía `governor`
- **Auth:** JWT contra Thalamus
- **Workers:** 13 crates independientes (DeepSeek, Anthropic, OpenAI, Gemini, Groq, Ollama, Brave, Serper, Tavily, DuckDuckGo, PubMed, SII Chile, iConstruye)

### Patrones de resiliencia

Circuit breaker, retry con backoff + jitter, bulkhead (límites de concurrencia), backpressure queue, idempotency keys, caché de respuestas, dead letter queue, warm-up, request dedup, graceful shutdown.
