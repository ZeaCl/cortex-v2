# Cortex v2 — Guía de Patrones de Resiliencia

Explicación detallada de cada patrón de resiliencia implementado en Cortex v2, ordenados de más crítico a más nice-to-have.

---

## 1. 🔴 Circuit Breaker (Interruptor de circuito)

**Problema:** Si DeepSeek está caído y le llegan 500 requests, cada uno va a esperar 60s de timeout. Son 500 × 60s = 8 horas de tiempo acumulado perdido. Además, el backend caído recibe aún más carga, empeorando la situación.

**Cómo funciona:**

```
Estado CLOSED (normal): todas las requests pasan
        │
        │ 3 fallos consecutivos
        ▼
Estado OPEN (cortado): rechaza requests inmediatamente por 30s
        │
        │ pasan los 30s
        ▼
Estado HALF-OPEN: deja pasar 1 request de prueba
        │
        ├─ éxito → vuelve a CLOSED
        └─ fallo → vuelve a OPEN por 30s más
```

**Ejemplo concreto:** DeepSeek falla 3 veces seguidas → el breaker se abre. Las próximas requests que pidan DeepSeek específicamente reciben error inmediato. Las que no especifican provider van directo a Gemini. A los 30s, se prueba 1 request a DeepSeek. Si responde bien, se reactiva. Si no, otros 30s de castigo.

**Por qué 3 strikes y no 1:** Un error aislado (timeout de red, pico momentáneo) no debe degradar un worker. 3 fallos consecutivos sí indican un problema real.

---

## 2. 🔴 Retry con Backoff Exponencial + Jitter

**Problema:** Sin jitter, imaginá 100 instancias del gateway que detectan que Thalamus volvió exactamente a los 30s. Las 100 hacen el request al mismo milisegundo → Thalamus recibe un pico 100× y se cae de nuevo. Esto se llama **thundering herd**.

**Cómo funciona:**

```
Intento 1: inmediato
  ↓ falla
Intento 2: esperar 1s ± 25% (entre 750ms y 1250ms)
  ↓ falla
Intento 3: esperar 2s ± 25% (entre 1.5s y 2.5s)
  ↓ falla
Intento 4: esperar 4s ± 25% (entre 3s y 5s)
  ↓ falla → se rinde
```

**El jitter hace que los reintentos se dispersen.** Sin jitter, 100 gateways reintentan todos a los 1s exactos → pico. Con jitter, se distribuyen entre 750ms y 1250ms → carga suave y uniforme.

**Dónde se aplica jitter en Cortex v2:**
- Recovery de workers degradados: `degradation_ttl_secs + jitter`
- Failover entre workers: 20-200ms de delay con jitter
- Refresh de JWKS de Thalamus: TTL + jitter para no pegarle todos al mismo tiempo

---

## 3. 🟡 Bulkhead (Mamparo estanco)

**El nombre viene de los barcos:** si chocás contra un iceberg, cerrás los mamparos para que el agua no inunde todo el barco. Mismo concepto en software.

**Problema:** El SII responde lento (30s por request). Si no hay límite, 100 requests al SII consumen todas las conexiones HTTP del gateway. De repente DeepSeek y Gemini también dejan de funcionar porque no hay conexiones disponibles — aunque ellos estén perfectos. Un worker lento **contamina** a todos los demás.

**Cómo funciona:**

```
Gateway: máximo 100 requests simultáneos en total (global)
  ├─ DeepSeek: máximo 20 simultáneos (per-worker)
  ├─ Gemini: máximo 20 simultáneos (per-worker)
  ├─ SII: máximo 20 simultáneos (per-worker)
  └─ iConstruye: máximo 20 simultáneos (per-worker)
```

**Ejemplo:** SII ya tiene 20 requests en vuelo y llega el 21°. El router **se saltea SII** y prueba el siguiente worker de tipo `tax`. Si no hay más workers, devuelve 503 inmediato. DeepSeek sigue funcionando perfecto porque su mamparo (bulkhead) está intacto — el agua del SII no lo tocó.

**Por qué 503 y no encolar:** Encolar requests indefinidamente crea la ilusión de disponibilidad mientras la latencia se degrada para todos. Es mejor rechazar rápido y que el cliente decida qué hacer (reintentar con backoff, mostrar mensaje, etc.).

---

## 4. 🟡 Backpressure (Contrapresión)

**Problema:** Sin backpressure, cuando el gateway está sobrecargado, las requests se acumulan en la cola TCP del sistema operativo. El cliente no recibe respuesta hasta que el buffer se llena y el SO rechaza la conexión. El cliente se queda colgado 30s sin saber qué pasó.

**Cómo funciona:**

```
Cola de backpressure: máximo 200 requests (FIFO)
  │
  ├─ Request 1-200: entran a la cola
  │   └─ Si pasan más de request_timeout_secs (30s) en la cola → 503
  │
  └─ Request 201+: 503 inmediato, sin entrar a la cola
```

**La diferencia con Bulkhead:** Bulkhead rechaza cuando hay demasiados requests **en ejecución**. Backpressure rechaza cuando hay demasiados requests **esperando**. Son complementarios: Bulkhead protege los recursos activos, Backpressure protege la cola de espera.

**¿Cuándo conviene habilitar backpressure?** Para servicios donde los bursts son normales (ej: todos los usuarios abren la app a las 9am). La cola absorbe el pico y lo procesa ordenadamente. Si los bursts son anómalos (ej: un bug dispara 5000 requests), mejor tenerlo deshabilitado y que el Bulkhead rechace directo.

---

## 5. 🟢 Idempotency Keys (Claves de idempotencia)

**Problema:** Un usuario hace clic en "Crear orden de compra" en iConstruye. La request llega al gateway, se envía a iConstruye, iConstruye crea la orden... pero la respuesta de "éxito" se pierde por un problema de red. El frontend no sabe si se creó o no, y reenvía la request. Resultado: **dos órdenes de compra duplicadas**, doble cobro al proveedor.

**Cómo funciona:**

```
POST /api/services/erp
Idempotency-Key: a1b2c3d4-e5f6-7890-abcd-ef1234567890

{
  "action": "crear_orden_compra",
  "payload": { "proveedor": "...", "items": [...] }
}
```

```
Primera request con key a1b2c3d4:
  Gateway: "no tengo esta key registrada"
  → envía a iConstruye
  → iConstruye crea la orden #1234
  → Gateway guarda (key, respuesta) en caché LRU por 24h
  → Respuesta: { "orden_id": 1234, "estado": "creada" }

  ⚠️ La respuesta se pierde por un problema de red

Segunda request con la misma key a1b2c3d4 (reenvío del frontend):
  Gateway: "ya tengo esta key, respuesta cacheada"
  → devuelve la respuesta inmediatamente
  → iConstruye NUNCA se entera de esta segunda request
  → Respuesta: { "orden_id": 1234, "estado": "creada" }
```

**Si la primera request todavía está en vuelo** y llega una segunda con la misma key, el gateway espera a que la primera termine y devuelve la misma respuesta a ambas. Esto se implementa con un `tokio::sync::oneshot` channel por key.

**Solo aplica a business workers** con `side_effects = true` en su config. LLM workers no lo necesitan — pedirle dos veces lo mismo a DeepSeek es inofensivo (y de hecho puede dar respuestas distintas, no es idempotente por naturaleza, pero no tiene side effects persistentes).

**Es el mismo patrón que usan Stripe y AWS** — no inventamos nada, aplicamos una práctica establecida.

---

## 6. 🟢 Response Caching (Caché de respuestas)

**Problema:** Consultar el RUT de Microsoft Chile al SII 1000 veces por día. Son 1000 llamadas a una API gubernamental lenta (2-5s por request) para obtener datos que no cambian hace 20 años. Desperdicio de tiempo, recursos, y riesgo de que el SII nos rate-limite.

**Cómo funciona:**

```
POST /api/services/tax
{
  "action": "consultar_rut",
  "payload": { "rut": "96.819.410-8" }
}
```

```
Primera consulta:
  Gateway: "no tengo esto en caché"
  → SII worker → respuesta en 3 segundos
  → Gateway guarda en caché con TTL de 1 hora
  → Respuesta: { "razon_social": "MICROSOFT CHILE SPA", ... }

Segunda consulta (10 segundos después):
  Gateway: "está en caché, TTL válido por 3590s más"
  → devuelve de caché en 0.5ms
  → SII: NUNCA se entera
  → Respuesta: misma

Tercera consulta (2 horas después):
  Gateway: "TTL expirado, fuera del caché"
  → SII worker → respuesta fresca → actualiza caché
```

**Configuración por worker y por acción:**

```toml
[cache]
enabled = true
max_entries = 10000
default_ttl_secs = 300

[workers.sii.cache]
ttl_secs = 3600           # RUT no cambia → 1 hora
max_entry_size_bytes = 10240

[workers.brave.cache]
ttl_secs = 600            # Búsquedas cambian más → 10 min
```

**Qué se cachea y qué no:**
- ✅ Acciones de solo lectura: `consultar_rut`, `buscar_proveedores`, búsquedas web
- ❌ Acciones con side effects: `crear_orden_compra`, `emitir_factura` — nunca se cachean
- ❌ LLM requests: por naturaleza no deterministicos, no tiene sentido cachearlos

**Implementación:** Se usa la crate `moka` que provee un caché concurrente con TTL y evicción LRU, optimizado para Rust async.

---

## 7. 🟢 Dead Letter Queue (Cola de mensajes muertos)

**Problema:** Un request de iConstruye para crear una orden de compra falla. Todos los workers de tipo `erp` están degradados (circuit breaker abierto). Si simplemente devolvemos error, **la orden de compra se pierde para siempre** — nadie sabe que alguien intentó crearla, el proveedor no recibe el pedido, y no hay registro de lo que pasó.

**Cómo funciona:**

```
Request: crear_orden_compra para iConstruye
  ↓ Intento con iConstruye-primary: HTTP 503
  ↓ Failover a iConstruye-secondary: timeout
  ↓ ¿Hay más workers erp? No.
  ↓ ¿El worker tiene side_effects = true? Sí
  ▼
  Dead Letter Queue
```

```
┌────┬──────────────┬──────────────────┬───────────────────────────┬──────────┐
│ ID │ Worker       │ Action           │ Payload                   │ Intentos │
├────┼──────────────┼──────────────────┼───────────────────────────┼──────────┤
│ 42 │ iconstruye   │ crear_orden      │ {"proveedor":"Matco",     │ 0/5      │
│    │              │                  │  "items":[...]}           │          │
│ 43 │ sii          │ emitir_factura   │ {"rut":"76.123.456-K",    │ 2/5      │
│    │              │                  │  "monto":1500000}         │          │
│ 51 │ sii          │ consultar_rut    │ {"rut":"96.819.410-8"}    │ 5/5 ❌   │
└────┴──────────────┴──────────────────┴───────────────────────────┴──────────┘
```

**Reintentos automáticos con backoff:**

| Intento | Delay |
|---------|-------|
| 1 | 1 minuto |
| 2 | 2 minutos |
| 3 | 4 minutos |
| 4 | 8 minutos |
| 5 | 16 minutos |

Si después de 5 intentos sigue fallando (como el ID #51), queda para revisión manual:

```bash
cortex dlq list                    # ver todas las entradas pendientes
cortex dlq show 42                 # ver detalle de una entrada
cortex dlq retry 42                # reintentar manualmente
cortex dlq discard 51              # descartar (ej: ya se procesó por otro medio)
cortex dlq purge                   # limpiar entradas ya procesadas
```

**Storage:** SQLite por defecto (sin dependencia externa, el archivo va en `/var/lib/cortex/dlq.db`). En producción, PostgreSQL para poder consultar desde otros servicios.

**Solo aplica a workers con `side_effects = true`.** Los workers LLM no usan DLQ — reintentar un chat request de hace 2 horas no tiene sentido.

---

## 8. 🟢 Warm-Up (Calentamiento)

**Problema:** El gateway arranca, el balanceador de carga ya le está mandando tráfico porque el puerto 4000 respondió, pero los workers todavía están estableciendo conexiones TCP, negociando TLS con los providers, etc. Las primeras 50-100 requests fallan o van lentísimas porque los workers no están listos.

**Cómo funciona:**

```toml
[workers.deepseek]
warmup = true
warmup_timeout_secs = 10
```

```
Secuencia de arranque:
  t=0s:   Gateway inicia servidor HTTP en puerto 4000
  t=0s:   Health check responde 503 ("starting")
  t=0s:   Corre health checks a todos los workers con warmup=true
  t=2s:   Gemini responde OK ✅
  t=4s:   DeepSeek responde OK ✅
  t=4s:   Todos los warmup workers listos
  t=4s:   Health check ahora responde 200 ("healthy")
  t=4.5s: Balanceador de carga detecta 200 y empieza a rutear tráfico
```

Si DeepSeek no responde en 10s (`warmup_timeout_secs`), el gateway arranca igual pero con DeepSeek marcado como `degraded`. Los demás workers funcionan normalmente.

**Workers sin warmup:** Workers como DuckDuckGo o Tavily que no requieren autenticación no necesitan warmup — pueden aceptar tráfico inmediatamente.

---

## 9. 🟢 Request Deduplication (Deduplicación por ventana temporal)

**Problema:** Un usuario está en un date picker. Cada vez que cambia la fecha, el frontend dispara un request. Si el usuario scrollea rápido por 10 fechas, se envían 10 requests en 200ms. 9 de ellas son basura — solo la última fecha seleccionada importa. Pero todas llegan al gateway y consumen recursos.

**Cómo funciona:**

```
Ventana de 300ms. Dos requests se consideran duplicadas si coinciden en:
  - user_sub (del JWT)
  - service_type
  - action
  - hash(payload)
```

```
t=0ms:    Request A (fecha=2024-01-01, usuario=123)
          → no hay duplicado → se ejecuta normalmente
          
t=50ms:   Request B (fecha=2024-01-02, usuario=123)
          → payload distinto (fecha cambió) → se ejecuta normalmente
          
t=100ms:  Request C (fecha=2024-01-01, usuario=123)
          → mismo payload que A, dentro de ventana de 300ms
          → DEDUP: espera a que A termine, devuelve misma respuesta
          
t=400ms:  Request D (fecha=2024-01-01, usuario=123)
          → mismo payload que A, pero fuera de la ventana de 300ms
          → se ejecuta normalmente
```

**Diferencia con Idempotency Keys:**

| | Dedup | Idempotency |
|---|---|---|
| **Quién la activa** | Automático (hash del payload) | Explícito (header del cliente) |
| **Ventana** | 300ms | 24 horas |
| **Objetivo** | Evitar requests duplicados accidentales | Garantizar exactly-once para operaciones críticas |
| **Alcance** | Todos los workers | Solo business workers con side_effects |

---

## 10. 🟢 Graceful Shutdown (Apagado elegante)

**Problema:** Kubernetes manda SIGTERM al pod (por deploy, scaling, o fallo). Si el gateway se muere inmediatamente, las requests en vuelo se cortan. Streaming requests dejan de emitir chunks a medio camino. Los clientes ven errores feos. La auditoría se pierde.

**Cómo funciona:**

```
1. Llega SIGTERM
2. Health check cambia a 503 ("shutting down")
3. Balanceador de carga detecta 503, deja de enviar tráfico nuevo
4. Gateway deja de aceptar nuevas conexiones TCP
5. Espera hasta shutdown_timeout_secs (default: 30s):
   ├─ Requests HTTP normales: espera a que terminen
   ├─ Streaming requests: espera a que emitan event:done, 
   │  o las cancela si exceden el timeout
   └─ Audit log pendiente: flush a PostgreSQL/SQLite
6. Deregister del service mesh de ZEA (si está configurado)
7. Process exit(0)
```

**Qué pasa si hay requests que no terminan a tiempo:**
- Requests normales: se cortan, el cliente recibe error de conexión
- Streaming: se emite un `event: error` con `{"error": "gateway_shutting_down"}` y se cierra
- DLQ entries pendientes: se hace flush al storage

---

## Composición final: el request completo

Así se compone un request desde que entra hasta que sale, pasando por todas las capas:

```
                    Request entrante
                         │
                         ▼
              ┌─────────────────────┐
              │ 1. DEDUP            │  ¿Mismo usuario+acción+payload en 300ms?
              │    (ventana 300ms)  │  → sí: devuelve respuesta del primero
              └─────────┬───────────┘  → no: sigue
                        ▼
              ┌─────────────────────┐
              │ 2. BULKHEAD         │  ¿Hay < 100 requests en vuelo?
              │    (global 100)     │  → no: 503 inmediato
              └─────────┬───────────┘  → sí: sigue
                        ▼
              ┌─────────────────────┐
              │ 3. BACKPRESSURE     │  ¿Cola FIFO < 200?
              │    (cola opcional)  │  → sí: encolar. no: 503 inmediato
              └─────────┬───────────┘  → sale de la cola cuando hay slot
                        ▼
              ┌─────────────────────┐
              │ 4. RATE LIMITER     │  ¿Usuario dentro de su cuota RPM?
              │    (GCRA)           │  → no: 429 con retry-after
              └─────────┬───────────┘  → sí: sigue
                        ▼
              ┌──────────────────────────────────────────────┐
              │ 5. ROUTER + CIRCUIT BREAKER + JITTER         │
              │                                              │
              │  Worker 1 ──→ ¿breaker abierto? → skip       │
              │       └─→ ¿breaker cerrado? → intentar       │
              │              │                               │
              │              ├─ éxito → 6. PER-WORKER BULKHEAD│
              │              │              ¿< 20 en vuelo?   │
              │              │              → sí: ejecutar    │
              │              │              → no: probar w2   │
              │              │                               │
              │              └─ fallo → JITTER(delay) → w2   │
              │                                              │
              │  Worker 2 ──→ ...                             │
              │  Worker 3 ──→ ...                             │
              └──────────────────────────────────────────────┘
                        ▼
              ┌─────────────────────┐
              │ 7. RETRY + TIMEOUT  │  Hasta 3 intentos con backoff
              │    + KEY ROTATION   │  1s→2s→4s ±25%, timeout por worker
              │                     │  Si 429: rotar API key y reintentar
              └─────────┬───────────┘
                        ▼
              ┌─────────────────────┐
              │ 8. IDEMPOTENCY      │  ¿Tiene Idempotency-Key?
              │    (solo business)  │  → sí: dedup por 24h o esperar
              └─────────┬───────────┘
                        ▼
              ┌─────────────────────┐
              │ 9. RESPONSE CACHE   │  ¿Read + cacheable + TTL válido?
              │    (solo lectura)   │  → sí: devuelve cacheado
              └─────────┬───────────┘
                        ▼
                   ┌─────────┐
                   │  ÉXITO  │ → respuesta al cliente
                   └─────────┘
                        │
                   ┌────┴────────────────────────┐
                   │ ¿Fallaron todos los workers? │
                   │                              │
                   │ side_effects = true          │
                   │ → 10. DEAD LETTER QUEUE      │
                   │    (reintentos automáticos   │
                   │     1m→2m→4m→8m→16m)        │
                   │                              │
                   │ side_effects = false         │
                   │ → error al cliente           │
                   └──────────────────────────────┘
```

---

## Configuración de referencia

```toml
# cortex.toml — sección completa de resiliencia

[gateway]
shutdown_timeout_secs = 30

[router]
strategy = "priority"
failover_enabled = true
degradation_threshold = 3
degradation_ttl_secs = 30
max_concurrent_requests = 100
max_concurrent_per_worker = 20
jitter_pct = 25
request_timeout_secs = 30
backpressure_queue_size = 200
backpressure_enabled = true

[cache]
enabled = true
max_entries = 10000
default_ttl_secs = 300

[dead_letter_queue]
enabled = true
storage = "sqlite"
path = "/var/lib/cortex/dlq.db"
max_retries = 5
retry_backoff_base_secs = 60

[connection_pool]
max_idle_per_host = 10
pool_max_idle_timeout = 90
pool_max_size = 50
tcp_keepalive = 60
connect_timeout = 10

[dedup]
enabled = true
window_ms = 300
```

---

## Resumen: qué patrón aplica a qué worker

| Patrón | LLM | Search | Business (SII, iConstruye) |
|--------|-----|--------|-----------------------------|
| Circuit Breaker | ✅ | ✅ | ✅ |
| Retry + Jitter | ✅ | ✅ | ✅ |
| Bulkhead | ✅ | ✅ | ✅ |
| Backpressure | Opcional | Opcional | Opcional |
| Idempotency Keys | ❌ | ❌ | ✅ |
| Response Caching | ❌ | ✅ (10min) | ✅ (1h para lecturas) |
| Dead Letter Queue | ❌ | ❌ | ✅ |
| Warm-Up | ✅ | Opcional | ✅ |
| Request Dedup | ✅ | ✅ | ✅ |
| Graceful Shutdown | ✅ | ✅ | ✅ |
