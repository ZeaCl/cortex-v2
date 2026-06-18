# Documento de Requisitos — Cortex v2 Universal Service Gateway

## Introducción

Cortex v2 es la segunda generación del gateway de servicios de la plataforma ZEA, reescrito desde cero en Rust. Evoluciona de ser un gateway exclusivamente orientado a LLMs a convertirse en un **enrutador universal de servicios**: cualquier API externa —modelos de lenguaje, búsquedas web, APIs gubernamentales, ERPs, o servicios internos de ZEA— se integra implementando un mismo trait (`Worker`) y se registra en el gateway. Cortex v2 no sabe qué hace cada worker, solo rutea requests, aplica failover, rate limiting, y recolecta telemetría.

La autenticación se delega completamente a **Thalamus** (el servicio de auth de ZEA): cada request incluye un JWT emitido por Thalamus. Cortex v2 valida el token, resuelve los scopes del usuario, y decide si el request puede enrutarse al worker solicitado. Los workers pueden usar credenciales del servidor (API keys configuradas en el gateway) o credenciales del usuario (OAuth tokens incluidos como claims en el JWT o resueltos desde Thalamus).

### Propósito

Proveer un punto de entrada unificado, resiliente y observable para todos los servicios externos e internos que consume la plataforma ZEA, eliminando la necesidad de que cada microservicio implemente su propia lógica de failover, rate limiting, autenticación y rotación de credenciales.

### Alcance

- Abstracción de workers vía trait para cualquier tipo de servicio (LLM, search, tax, ERP, etc.)
- Workers LLM: DeepSeek, Anthropic, OpenAI, Gemini, Groq, Ollama
- Workers de búsqueda: Brave, Serper, Tavily, DuckDuckGo, PubMed
- Workers de negocio: SII Chile, iConstruye
- Integración con Thalamus para autenticación vía JWT
- Streaming de respuestas (SSE, JSON streaming)
- Failover automático con resiliencia 3-strikes
- Rate limiting global, por worker y por usuario/API key
- Rotación de API keys con estrategias configurables
- HTTP API compatible con OpenAI y endpoints REST nativos
- CLI de administración
- Workers desacoplados como crates independientes

### Propuesta de Valor

Un gateway universal en Rust que cualquier servicio de ZEA puede llamar con una interfaz unificada, sin preocuparse por qué provider está disponible, cómo manejar rate limits, o cómo rotar credenciales. Los workers son crates independientes que cualquier equipo puede desarrollar y publicar sin tocar el core del gateway.

---

## Requisitos

### Requisito 1: Abstracción de Workers — Trait Universal

**Historia de Usuario:** Como desarrollador, quiero agregar soporte para un nuevo servicio externo implementando un solo trait de Rust, para no tener que modificar el core del gateway.

#### Criterios de Aceptación

1. WHEN un desarrollador implementa el trait `Worker` para un nuevo servicio THEN the system SHALL aceptar el worker sin requerir cambios en el core del gateway
2. IF un worker declara su `service_type` como `"llm"` THEN the system SHALL enrutar requests de chat/completion a ese worker
3. IF un worker declara su `service_type` como `"search"` THEN the system SHALL enrutar requests de búsqueda a ese worker
4. IF un worker declara un `service_type` personalizado THEN the system SHALL exponer un endpoint genérico `/api/services/{service_type}` que enrute a los workers coincidentes
5. WHEN un worker implementa `execute_stream` THEN the system SHALL soportar respuestas con streaming a través de ese worker
6. IF un worker NO implementa `execute_stream` THEN the system SHALL usar `execute` como fallback y devolver la respuesta como un solo payload
7. WHEN un worker declara capacidades como `["chat", "tools", "reasoning"]` THEN the system SHALL exponer esas capacidades en el endpoint `/api/health/detailed`
8. WHERE un worker tiene nivel de prioridad N THEN the system SHALL preferir workers con valores de prioridad más bajos al auto-seleccionar

---

### Requisito 2: Workers LLM — DeepSeek, Anthropic, OpenAI, Gemini, Groq, Ollama

**Historia de Usuario:** Como servicio de ZEA, quiero enviar requests de chat completion a cualquier proveedor LLM a través de una sola API, para poder cambiar de proveedor sin modificar mi código.

#### Criterios de Aceptación

1. WHEN se envía un request a `POST /api/chat` con `provider: "deepseek"` THEN the system SHALL enrutar el request al worker de DeepSeek usando las API keys configuradas
2. WHEN se envía un request con `model: "deepseek-chat"` THEN the system SHALL seleccionar automáticamente el worker de DeepSeek
3. IF el worker de DeepSeek devuelve un error THEN the system SHALL intentar failover al siguiente worker LLM disponible según prioridad
4. WHEN un request de chat incluye `stream: true` THEN the system SHALL devolver eventos SSE para todos los workers LLM (DeepSeek, Anthropic, OpenAI, Gemini, Groq)
5. IF el worker de Ollama está disponible THEN the system SHALL preferirlo para requests locales cuando se especifica `provider: "ollama"` o se auto-detecta
6. WHEN un request incluye `tools` en el cuerpo THEN the system SHALL enrutar a un worker cuyas capacidades incluyan `"tools"`
7. WHERE un worker LLM soporta pensamiento extendido o razonamiento THEN the system SHALL reenviar el parámetro `thinking` a ese worker
8. WHEN una respuesta con streaming se completa THEN the system SHALL emitir un evento SSE `done` con metadata (modelo usado, nombre del worker, cadena de failover, info de rate limit)

---

### Requisito 3: Workers de Búsqueda — Brave, Serper, Tavily, DuckDuckGo, PubMed

**Historia de Usuario:** Como servicio de ZEA, quiero realizar búsquedas web y académicas a través de una API unificada, para poder consultar múltiples proveedores de búsqueda con failover automático.

#### Criterios de Aceptación

1. WHEN se envía un request a `POST /api/search` con `query` y `provider` opcional THEN the system SHALL enrutar al worker de búsqueda especificado o auto-seleccionar uno
2. IF no hay workers de búsqueda disponibles THEN the system SHALL devolver error 503 con detalles sobre qué workers se intentaron
3. WHEN una búsqueda se completa exitosamente THEN the system SHALL devolver resultados en formato unificado: `[{title, url, snippet, source}]`
4. WHERE se usa DuckDuckGo THEN the system SHALL no requerir API key
5. WHEN se consulta PubMed THEN the system SHALL soportar parámetros `max_results` y `sort`

---

### Requisito 4: Workers de Negocio — SII Chile, iConstruye

**Historia de Usuario:** Como servicio de ZEA en el dominio de negocio, quiero consultar APIs del gobierno chileno (SII) y APIs de ERP de construcción (iConstruye) a través del mismo gateway, para no tener que manejar clientes HTTP, tokens de autenticación y rate limiting separados para cada uno.

#### Criterios de Aceptación

1. WHEN se envía un request a `POST /api/services/tax` con `action: "consultar_rut"` y `rut: "XX.XXX.XXX-X"` THEN the system SHALL enrutar al worker del SII y devolver información del contribuyente
2. WHEN se envía un request a `POST /api/services/tax` con `action: "consultar_estado_tributario"` THEN the system SHALL devolver el estado fiscal actual del contribuyente
3. IF la API del SII devuelve un error de rate limit THEN the system SHALL hacer failover a un worker secundario del SII o devolver error 429 con retry-after
4. WHEN se envía un request a `POST /api/services/erp` con `action: "buscar_proveedores"` y `query` THEN the system SHALL enrutar al worker de iConstruye y devolver proveedores coincidentes
5. WHEN se envía un request a `POST /api/services/erp` con `action: "crear_orden_compra"` THEN the system SHALL enrutar al worker de iConstruye y crear una orden de compra
6. IF el worker de iConstruye encuentra un error de autenticación THEN the system SHALL intentar refrescar credenciales vía Thalamus antes de devolver un error
7. WHERE se usan workers de negocio THEN the system SHALL registrar todos los requests para propósitos de auditoría

---

### Requisito 5: Integración con Autenticación de Thalamus

**Historia de Usuario:** Como usuario de la plataforma ZEA, quiero autenticarme una vez con Thalamus y usar esa misma identidad para acceder a todos los servicios a través de Cortex v2, sin manejar API keys separadas.

#### Criterios de Aceptación

1. WHEN un request llega al gateway THEN the system SHALL validar el JWT en el header `Authorization: Bearer <jwt>` contra la clave pública de Thalamus
2. IF el JWT está expirado, es inválido o no existe THEN the system SHALL devolver 401 con `{"error": "unauthorized", "message": "..."}` y no enrutar ningún request
3. WHEN un request incluye un JWT válido THEN the system SHALL extraer los claims `sub`, `org_id` y `scopes` y ponerlos a disposición de los workers
4. IF el JWT incluye credenciales OAuth como claims (access_token, refresh_token, provider) THEN the system SHALL pasar esas credenciales al worker en lugar de usar API keys del servidor
5. WHERE un worker requiere API keys del servidor THEN the system SHALL usar variables de entorno o archivos de configuración sin exponerlas al usuario
6. WHEN el token OAuth de un usuario está expirado THEN the system SHALL devolver error 401 con detalles y no intentar refrescarlo (el refresh lo maneja Thalamus, no Cortex)
7. IF el endpoint de clave pública de Thalamus está inaccesible THEN the system SHALL usar una versión cacheada de la clave por hasta 1 hora antes de fallar
8. WHEN un request de worker se completa THEN the system SHALL registrar la identidad del usuario para propósitos de auditoría

---

### Requisito 6: Enrutamiento, Failover y Resiliencia

**Historia de Usuario:** Como operador de plataforma, quiero que el gateway maneje automáticamente las fallas de los workers sin intervención manual, para que los servicios sigan disponibles incluso cuando proveedores individuales están caídos.

#### Criterios de Aceptación

1. WHEN un worker falla con un error THEN the system SHALL incrementar su contador de fallos
2. IF un worker acumula 3 fallos consecutivos THEN the system SHALL marcarlo como `degraded` y excluirlo del enrutamiento durante 30 segundos
3. WHEN un worker degradado completa un health check exitosamente THEN the system SHALL restaurarlo al estado `active`
4. WHERE un request especifica un proveedor particular y ese proveedor está degradado THEN the system SHALL devolver un error en lugar de enrutar silenciosamente a otro proveedor
5. IF un request NO especifica un proveedor THEN the system SHALL seleccionar el worker disponible de mayor prioridad e intentar failover por todos los workers disponibles antes de devolver un error
6. WHEN ocurre failover THEN the system SHALL incluir la cadena de failover en la metadata de la respuesta
7. IF todos los workers de un service_type están degradados THEN the system SHALL devolver error 503 con la lista de workers intentados
8. WHERE un worker devuelve HTTP 429 (rate limited) THEN the system SHALL marcarlo inmediatamente como degradado sin esperar los 3 strikes

---

### Requisito 7: Gestión y Rotación de API Keys

**Historia de Usuario:** Como operador de plataforma, quiero configurar múltiples API keys por proveedor y que el gateway las rote automáticamente, para maximizar el throughput y manejar rate limits sin intervención.

#### Criterios de Aceptación

1. WHEN se configuran múltiples API keys para un worker THEN the system SHALL soportar estrategias de rotación: `round_robin`, `least_used` y `random`
2. IF un worker encuentra HTTP 429 con una API key THEN the system SHALL rotar a la siguiente key en el mismo worker antes de intentar failover a otro worker
3. WHEN una API key es rate-limited THEN the system SHALL bloquear esa key durante 15 minutos antes de reintentar
4. WHERE un worker ha agotado todas sus API keys THEN the system SHALL marcar el worker completo como degradado
5. WHEN una API key tiene éxito THEN the system SHALL actualizar sus estadísticas de uso para la estrategia `least_used`
6. IF solo hay una API key configurada para un worker THEN the system SHALL omitir la lógica de rotación y usar esa key directamente

---

### Requisito 8: Rate Limiting

**Historia de Usuario:** Como operador de plataforma, quiero aplicar rate limits globales y por usuario para proteger las cuotas de APIs upstream y garantizar uso justo.

#### Criterios de Aceptación

1. WHEN el gateway inicia THEN the system SHALL configurar un rate limit global desde `CORTEX_GLOBAL_RATE_LIMIT` (default: 1000 requests/minuto)
2. WHERE un usuario excede su rate limit por usuario THEN the system SHALL devolver 429 con header `retry-after` y mensaje de error
3. IF un worker específico tiene un rate limit más estricto (ej: Groq capa gratuita: 30 RPM) THEN the system SHALL aplicar ese límite antes de enrutar al worker
4. WHEN los tokens de rate limit se consumen asincrónicamente THEN the system SHALL usar el algoritmo GCRA para manejo preciso de ráfagas
5. IF el rate limit global se alcanza THEN the system SHALL devolver 429 a todos los usuarios independientemente de su cuota individual

---

### Requisito 9: Soporte de Streaming

**Historia de Usuario:** Como servicio de ZEA que consume respuestas LLM, quiero recibir tokens a medida que se generan, para mostrar salida progresiva al usuario.

#### Criterios de Aceptación

1. WHEN un request incluye `stream: true` THEN the system SHALL devolver `Content-Type: text/event-stream` y emitir eventos SSE
2. IF el worker upstream soporta streaming nativo THEN the system SHALL retransmitir los chunks a medida que llegan sin almacenar la respuesta completa
3. WHEN el stream se interrumpe por un error de conexión THEN the system SHALL intentar failover al siguiente worker y reanudar el streaming desde el mismo contexto de mensajes
4. WHERE el stream se completa exitosamente THEN the system SHALL emitir un `event: done` final con `{"done": true, "model": "...", "worker": "...", "usage": {...}}`
5. IF un worker NO soporta streaming nativo THEN the system SHALL almacenar la respuesta completa y emitirla como un solo evento SSE data seguido de done
6. WHEN el cliente se desconecta durante el streaming THEN the system SHALL cancelar el request upstream para evitar desperdiciar tokens

---

### Requisito 10: API HTTP y Compatibilidad con OpenAI

**Historia de Usuario:** Como desarrollador que se integra con Cortex v2, quiero una API compatible con el formato de chat completions de OpenAI, para que herramientas y SDKs existentes funcionen sin modificaciones.

#### Criterios de Aceptación

1. WHEN se envía un request POST a `/v1/chat/completions` con cuerpo en formato OpenAI THEN the system SHALL procesarlo de forma idéntica a `/api/chat`
2. WHEN se envía un request GET a `/v1/models` THEN the system SHALL devolver una lista de modelos disponibles en formato OpenAI
3. IF el cuerpo del request incluye `model: "deepseek-chat"` THEN the system SHALL enrutar a DeepSeek sin requerir un campo `provider` separado
4. WHEN se envía un request GET a `/api/health` THEN the system SHALL devolver `{"status": "healthy|degraded", "workers": N, "available": N}`
5. WHEN se envía un request GET a `/api/health/detailed` THEN the system SHALL devolver estado por worker, capacidades, modelos activos e información de fallback
6. WHERE se envía un request a `/api/stats` THEN the system SHALL devolver métricas agregadas: requests total/completados/fallidos, latencia promedio, tokens procesados
7. WHEN se envía un request GET a `/api/services` THEN the system SHALL devolver la lista de tipos de servicio registrados y sus workers disponibles

---

### Requisito 11: Observabilidad — Métricas, Trazas y Logging

**Historia de Usuario:** Como operador de plataforma, quiero monitorear el rendimiento del gateway, seguir el uso por proveedor y usuario, y depurar problemas con trazabilidad distribuida.

#### Criterios de Aceptación

1. WHEN el gateway inicia THEN the system SHALL exponer un endpoint de métricas Prometheus en `/metrics`
2. WHERE se procesa un request THEN the system SHALL emitir métricas de: conteo de requests, histograma de latencia, conteo de errores, tokens procesados (para workers LLM), por worker y por usuario
3. IF OpenTelemetry está habilitado THEN the system SHALL propagar contextos de traza a los workers upstream y exportar spans al colector configurado
4. WHEN ocurre un evento significativo (worker degradado, failover, rate limit alcanzado) THEN the system SHALL emitir logs estructurados en formato JSON a stdout
5. WHERE el nivel de log está configurado como `debug` THEN the system SHALL registrar cuerpos completos de request/response (excluyendo campos sensibles como API keys y tokens)

---

### Requisito 12: Configuración y Descubrimiento de Workers

**Historia de Usuario:** Como operador de plataforma, quiero configurar Cortex v2 mediante variables de entorno y archivos de configuración, con workers auto-descubiertos desde la configuración, para que el despliegue sea directo y repetible.

#### Criterios de Aceptación

1. WHEN el gateway inicia THEN the system SHALL cargar configuración desde variables de entorno, luego desde `cortex.toml`, con las env vars teniendo precedencia
2. IF `CORTEX_CONFIG_PATH` está definido THEN the system SHALL cargar configuración desde esa ruta en lugar del default
3. WHEN una sección de worker está presente en la config (ej: `[workers.deepseek]`) THEN the system SHALL instanciar ese worker con las API keys, modelo y timeout especificados
4. IF un worker tiene `enabled = false` en la config THEN the system SHALL omitir la carga de ese worker
5. WHERE una API key se especifica como referencia a env var (`${DEEPSEEK_API_KEY}`) THEN the system SHALL resolverla desde el entorno al iniciar
6. WHEN un worker falla al inicializarse por configuración faltante THEN the system SHALL registrar una advertencia y continuar sin ese worker, en lugar de fallar

---

### Requisito 13: Herramienta CLI de Administración

**Historia de Usuario:** Como operador de plataforma, quiero una herramienta CLI para verificar el estado del gateway, listar workers, probar requests y gestionar configuración sin escribir requests HTTP manualmente.

#### Criterios de Aceptación

1. WHEN se ejecuta `cortex status` THEN the system SHALL mostrar versión del gateway, uptime, workers conectados y resumen de salud
2. WHEN se ejecuta `cortex workers list` THEN the system SHALL mostrar todos los workers registrados con nombre, tipo, estado y modelo
3. WHEN se ejecuta `cortex test chat --provider deepseek "Hola"` THEN the system SHALL enviar un request de chat y hacer stream de la respuesta a stdout
4. WHEN se ejecuta `cortex test search "programación Elixir"` THEN the system SHALL enviar un request de búsqueda y mostrar resultados
5. WHERE se solicita una recarga de configuración vía `cortex config reload` THEN the system SHALL recargar la configuración de workers sin interrumpir requests en vuelo

---

### Requisito 14: Despliegue

**Historia de Usuario:** Como ingeniero DevOps, quiero desplegar Cortex v2 como un solo binario estático en un contenedor Docker, para que el despliegue sea rápido y reproducible.

#### Criterios de Aceptación

1. WHEN se ejecuta `cargo build --release` THEN the system SHALL producir un solo binario estático
2. IF el binario se compila con target `musl` THEN the system SHALL ejecutarse en un contenedor Docker `scratch` sin dependencias de runtime
3. WHEN el contenedor Docker inicia THEN the system SHALL pasar los health checks dentro de los primeros 10 segundos
4. WHERE el gateway se despliega junto a otros servicios ZEA THEN the system SHALL registrarse con el service mesh de ZEA para descubrimiento

---

### Requisito 15: Circuit Breaker

**Historia de Usuario:** Como operador de plataforma, quiero que el gateway deje de enviar requests automáticamente a workers que están fallando y reanude solo después de que se recuperen, para prevenir fallas en cascada y no desperdiciar recursos en requests condenados al fracaso.

#### Criterios de Aceptación

1. WHEN un worker falla 3 requests consecutivos THEN the system SHALL abrir el circuit breaker y rechazar nuevos requests a ese worker inmediatamente sin intentar la llamada upstream
2. WHILE el circuit breaker está abierto THEN the system SHALL mantenerlo abierto durante `degradation_ttl_secs` (default: 30s) antes de pasar a half-open
3. WHEN el cooldown expira THEN the system SHALL pasar a half-open y permitir exactamente 1 request de prueba
4. IF el request de prueba tiene éxito THEN the system SHALL cerrar el circuit breaker y reanudar el enrutamiento normal a ese worker
5. IF el request de prueba falla THEN the system SHALL reabrir el circuit breaker por otro período completo de cooldown
6. WHERE un cliente solicita explícitamente un proveedor cuyo circuit breaker está abierto THEN the system SHALL devolver un error en lugar de enrutar silenciosamente a otro proveedor
7. WHEN un request tiene éxito después de cualquier número de fallos previos THEN the system SHALL reiniciar el contador de fallos a 0

---

### Requisito 16: Reintentos con Backoff Exponencial y Jitter

**Historia de Usuario:** Como operador de plataforma, quiero que los requests fallidos se reintenten inteligentemente con demoras aleatorias, para absorber fallas transitorias sin crear problemas de estampida cuando múltiples instancias del gateway se recuperan simultáneamente.

#### Criterios de Aceptación

1. WHEN un request HTTP a un worker upstream falla con error 5xx o error de red THEN the system SHALL reintentar hasta 3 veces con backoff exponencial (demos base: 1s → 2s → 4s)
2. IF se aplica una demora de reintento THEN the system SHALL agregar jitter aleatorio de ±25% para prevenir reintentos sincronizados entre instancias
3. WHEN se programa un webhook o refresh de JWKS desde Thalamus THEN the system SHALL aplicar jitter al intervalo de refresh para evitar que todas las instancias golpeen Thalamus simultáneamente
4. IF un worker está degradado y entra en estado half-open THEN the system SHALL aplicar jitter a la expiración del cooldown para que múltiples instancias del gateway no prueben el worker al mismo instante
5. WHEN ocurre failover entre workers THEN the system SHALL insertar una demora con jitter de 20-200ms entre intentos para evitar saturar al siguiente worker

---

### Requisito 17: Bulkhead — Límite de Concurrencia

**Historia de Usuario:** Como operador de plataforma, quiero limitar cuántos requests concurrentes puede manejar cada worker y el gateway en su totalidad, para que un worker lento o saturado no consuma todos los recursos del sistema y ahogue a otros workers sanos.

#### Criterios de Aceptación

1. WHEN el gateway inicia THEN the system SHALL aplicar un límite de concurrencia global (`max_concurrent_requests`, default: 100) sobre requests en vuelo
2. IF se alcanza el límite de concurrencia global THEN the system SHALL rechazar nuevos requests con HTTP 503 inmediatamente sin encolarlos
3. WHEN se selecciona un worker para enrutamiento THEN the system SHALL verificar su límite de concurrencia por worker (`max_concurrent_per_worker`, default: 20)
4. IF un worker ha alcanzado su límite de concurrencia THEN the system SHALL omitir ese worker e intentar con el siguiente worker disponible del mismo service_type
5. WHERE todos los workers de un service_type están en su límite de concurrencia THEN the system SHALL devolver 503 con detalles de qué workers estaban a capacidad
6. WHEN un request se completa o falla THEN the system SHALL liberar su slot de concurrencia inmediatamente

---

### Requisito 18: Cola de Backpressure

**Historia de Usuario:** Como operador de plataforma, quiero opcionalmente absorber ráfagas cortas de tráfico en una cola acotada, para que los picos transitorios se absorban sin rechazar requests, pero protegiendo el sistema de encolamiento ilimitado.

#### Criterios de Aceptación

1. WHEN `backpressure_enabled = true` y el bulkhead global está saturado THEN the system SHALL encolar requests en una cola FIFO acotada de tamaño `backpressure_queue_size` (default: 200)
2. IF la cola de backpressure está llena THEN the system SHALL rechazar requests adicionales con HTTP 503 inmediatamente
3. WHILE un request está en la cola de backpressure por más de `request_timeout_secs` THEN the system SHALL descartarlo y devolver 503
4. WHEN un slot de concurrencia se libera THEN the system SHALL desencolar el request más antiguo de la cola de backpressure

---

### Requisito 19: Claves de Idempotencia

**Historia de Usuario:** Como desarrollador que integra APIs de negocio a través de Cortex v2, quiero garantizar ejecución exactamente-una-vez para operaciones críticas, para que los reintentos de red no creen efectos secundarios duplicados como órdenes de compra dobles o declaraciones de impuestos repetidas.

#### Criterios de Aceptación

1. WHEN un request incluye un header `Idempotency-Key` THEN the system SHALL rastrear el request por esa clave durante `idempotency_ttl_secs` (default: 24 horas)
2. IF llega un request con una Idempotency-Key que tiene una respuesta exitosa cacheada THEN the system SHALL devolver la respuesta cacheada sin llamar al worker upstream
3. IF llega un request con una Idempotency-Key que está actualmente en vuelo THEN the system SHALL esperar a que el request original termine y devolver la misma respuesta a ambos llamadores
4. WHEN el TTL de idempotencia expira THEN the system SHALL tratar un nuevo request con la misma clave como un request nuevo
5. WHERE un worker tiene `side_effects = true` en su configuración THEN the system SHALL requerir Idempotency-Key para acciones mutantes
6. IF un request mutante a un worker con side_effects no incluye Idempotency-Key THEN the system SHALL devolver 400 con un mensaje de error explicando el requisito

---

### Requisito 20: Caché de Respuestas

**Historia de Usuario:** Como desarrollador que consume APIs de solo lectura a través de Cortex v2, quiero que los datos solicitados frecuentemente se sirvan desde caché, para reducir la carga upstream y minimizar la latencia de respuesta.

#### Criterios de Aceptación

1. WHEN un worker declara acciones como cacheables vía `cacheable_actions()` THEN the system SHALL cachear respuestas exitosas para esas acciones
2. IF existe una respuesta cacheada y su TTL no ha expirado THEN the system SHALL devolver la respuesta cacheada sin llamar al worker upstream
3. WHEN el TTL expira THEN the system SHALL desalojar la entrada de caché y obtener datos frescos en el próximo request
4. WHERE una acción de worker tiene efectos secundarios (ej: `crear_orden_compra`, `emitir_factura`) THEN the system SHALL nunca cachear la respuesta
5. IF la caché alcanza `max_entries` THEN the system SHALL desalojar la entrada menos recientemente usada
6. WHEN el gateway se apaga THEN the system SHALL descartar la caché en memoria (el arranque en frío en el próximo inicio es aceptable)

---

### Requisito 21: Cola de Mensajes Muertos (Dead Letter Queue)

**Historia de Usuario:** Como operador de plataforma, quiero que los requests fallidos a APIs críticas de negocio se persistan y reintenten automáticamente, para que ninguna orden de compra, declaración de impuestos o presentación regulatoria se pierda silenciosamente.

#### Criterios de Aceptación

1. WHEN todos los workers de un service_type con `side_effects = true` fallan al procesar un request después del failover completo THEN the system SHALL persistir el request en la dead letter queue
2. WHILE un request está en la dead letter queue THEN the system SHALL reintentarlo automáticamente con backoff exponencial (1min → 2min → 4min → 8min → 16min) hasta `max_retries` (default: 5)
3. IF una entrada de DLQ agota todos los reintentos THEN the system SHALL marcarla como `failed_permanent` y requerir intervención manual
4. WHEN se ejecuta el comando CLI `cortex dlq list` THEN the system SHALL mostrar todas las entradas pendientes y fallidas de la DLQ
5. WHEN se ejecuta el comando CLI `cortex dlq retry <id>` THEN the system SHALL reintentar inmediatamente la entrada especificada
6. WHERE el almacenamiento de DLQ es SQLite o PostgreSQL THEN the system SHALL persistir las entradas de forma duradera antes de devolver el error al cliente

---

### Requisito 22: Calentamiento de Workers (Warm-Up)

**Historia de Usuario:** Como operador de plataforma, quiero que el gateway verifique la salud de los workers antes de aceptar tráfico de producción, para que el balanceador de carga nunca enrute requests a un gateway cuyos workers aún no están listos.

#### Criterios de Aceptación

1. WHEN el gateway inicia Y los workers están configurados con `warmup = true` THEN the system SHALL realizar health checks contra esos workers antes de marcar el gateway como saludable
2. WHILE el calentamiento está en progreso THEN el endpoint `/api/health` SHALL devolver estado `starting` con HTTP 503
3. WHEN todos los workers de calentamiento pasan sus health checks O `warmup_timeout_secs` expira THEN the system SHALL transicionar `/api/health` para reflejar el estado real
4. IF un worker de calentamiento falla su health check y el timeout expira THEN the system SHALL iniciar con ese worker marcado como `degraded` y el gateway como `degraded`
5. WHERE un worker tiene `warmup = false` THEN the system SHALL aceptar tráfico inmediatamente sin esperar a ese worker

---

### Requisito 23: Deduplicación de Requests

**Historia de Usuario:** Como desarrollador de una aplicación orientada a UI, quiero que el gateway deduplique automáticamente requests idénticos enviados en rápida sucesión, para que fallos de UI (cambios rápidos de fecha, dobles clics en botones) no desperdicien recursos upstream.

#### Criterios de Aceptación

1. WHEN dos requests llegan dentro de `dedup.window_ms` (default: 300ms) con el mismo `user_sub`, `service_type`, `action` y hash de payload THEN the system SHALL tratar el segundo como duplicado
2. IF el request original todavía está en vuelo THEN the system SHALL esperar a que termine y devolver la misma respuesta al duplicado
3. IF el request original ya se completó THEN the system SHALL devolver la respuesta cacheada al duplicado
4. WHEN la ventana de deduplicación expira THEN the system SHALL procesar un nuevo request idéntico normalmente
5. WHERE un request incluye un header `Idempotency-Key` THEN el mecanismo de idempotencia SHALL tener precedencia sobre la deduplicación de ventana corta

---

### Requisito 24: Apagado Elegante (Graceful Shutdown)

**Historia de Usuario:** Como ingeniero DevOps, quiero que el gateway drene los requests en vuelo antes de apagarse, para que los deploys y terminaciones de pods no interrumpan sesiones de usuario activas ni corrompan respuestas de streaming.

#### Criterios de Aceptación

1. WHEN el gateway recibe SIGTERM THEN the system SHALL dejar de aceptar nuevas conexiones y devolver 503 en el endpoint de salud
2. WHILE los requests en vuelo se están drenando THEN the system SHALL esperar hasta `shutdown_timeout_secs` (default: 30s) para que se completen
3. IF los requests en vuelo no se completan dentro de `shutdown_timeout_secs` THEN the system SHALL cancelarlos y emitir eventos de error para requests de streaming
4. WHEN todos los requests en vuelo se han drenado o el timeout expira THEN the system SHALL hacer flush de las entradas pendientes del audit log y DLQ al almacenamiento
5. WHERE el gateway está registrado con el service mesh de ZEA THEN the system SHALL deregistrarse antes de comenzar el drenaje
6. AFTER el drenaje se completa THEN the system SHALL salir con código 0

---

### Requisito 25: Pool de Conexiones

**Historia de Usuario:** Como operador de plataforma, quiero que el cliente HTTP reutilice conexiones eficientemente entre requests, para minimizar la sobrecarga de establecimiento de conexiones y no saturar a los proveedores upstream con handshakes TCP redundantes.

#### Criterios de Aceptación

1. WHEN el gateway inicia THEN the system SHALL configurar el cliente HTTP compartido con `max_idle_per_host` conexiones keep-alive por host upstream
2. IF una conexión ha estado inactiva por más de `pool_max_idle_timeout` THEN the system SHALL cerrarla
3. WHEN el número total de conexiones en el pool alcanza `pool_max_size` THEN the system SHALL esperar a que una conexión esté disponible en lugar de abrir una nueva
4. WHERE un worker tiene requisitos de conexión personalizados THEN the system SHALL permitir overrides por worker para `max_idle_per_host` y `connect_timeout`
5. WHEN se establece una conexión TCP a un host upstream THEN the system SHALL aplicar un límite de `connect_timeout`

---

## Requisitos No Funcionales

### Rendimiento
- **NFR-P1**: El gateway SHALL agregar no más de 5ms de overhead a la latencia de request (excluyendo latencia del worker upstream)
- **NFR-P2**: El gateway SHALL soportar al menos 10,000 conexiones concurrentes para streaming
- **NFR-P3**: El uso de memoria en reposo SHALL no exceder 50MB

### Seguridad
- **NFR-S1**: Las API keys SHALL nunca aparecer en logs, métricas o mensajes de error — SHALL enmascararse como `sk-...xxxx`
- **NFR-S2**: Toda comunicación con Thalamus SHALL usar HTTPS
- **NFR-S3**: El gateway SHALL validar las firmas JWT contra la clave pública de Thalamus en cada request

### Confiabilidad
- **NFR-R1**: El gateway SHALL alcanzar 99.9% de uptime excluyendo fallas de proveedores upstream
- **NFR-R2**: La degradación de workers SHALL recuperarse automáticamente sin intervención del operador
- **NFR-R3**: Los requests en vuelo SHALL completarse correctamente durante recargas de configuración
- **NFR-R4**: WHEN el gateway recibe SIGTERM THEN the system SHALL dejar de aceptar nuevos requests y drenar requests en vuelo dentro de `shutdown_timeout_secs` antes de salir
- **NFR-R5**: WHEN los requests concurrentes exceden `max_concurrent_requests` THEN the system SHALL devolver HTTP 503 inmediatamente (Bulkhead)
- **NFR-R6**: IF un worker de negocio falla todos los intentos de failover THEN the system SHALL persistir el request en la dead letter queue para revisión manual
- **NFR-R7**: WHEN ocurre failover entre workers THEN the system SHALL aplicar jitter (±25%) a las demoras de reintento para prevenir estampida

### Resiliencia
- **NFR-X1**: WHEN un request incluye un header `Idempotency-Key` THEN the system SHALL deduplicar requests idénticos dentro de `idempotency_ttl_secs`
- **NFR-X2**: IF existe una respuesta cacheada para una operación de lectura THEN the system SHALL devolverla sin llamar al worker upstream
- **NFR-X3**: WHEN el gateway inicia THEN the system SHALL hacer health-check a todos los workers con `warmup: true` antes de aceptar tráfico
- **NFR-X4**: WHERE `backpressure_enabled = true` THEN the system SHALL encolar hasta `backpressure_queue_size` requests y rechazar el excedente con 503
- **NFR-X5**: IF un worker está saturado (límite de concurrencia por worker alcanzado) THEN the system SHALL omitirlo e intentar con el siguiente worker disponible

### Mantenibilidad
- **NFR-M1**: Agregar un nuevo worker SHALL requerir implementar un solo trait y un bloque de configuración — sin cambios al core del gateway
- **NFR-M2**: Cada worker SHALL vivir en su propio crate, versionado y publicable independientemente
- **NFR-M3**: El core del gateway SHALL tener cobertura de tests > 80%

---

## Casos Borde

1. **Un worker falla al inicializar al iniciar el gateway**: El gateway inicia con los workers restantes, registra advertencia, continúa operando. Workers degradados pueden agregarse después vía recarga de configuración.
2. **Todos los workers de un service_type están degradados**: Devuelve 503 inmediatamente sin reintentar, incluye todos los nombres de workers y razones en la respuesta.
3. **Thalamus está inaccesible al iniciar**: El gateway reintenta cada 30 segundos con backoff exponencial (máximo 5 minutos). Usa JWKS cacheado si está disponible por hasta 1 hora.
4. **El JWT expira durante el streaming**: El gateway valida el JWT solo al inicio del stream. La expiración durante el streaming no interrumpe el stream.
5. **Un worker upstream devuelve un stream parcial y se desconecta**: El gateway intenta failover al siguiente worker, reenvía el array completo de mensajes y reanuda el streaming.
6. **El cliente se desconecta durante el streaming**: El gateway cancela el request upstream para evitar desperdiciar tokens.
7. **El archivo de configuración está malformado**: El gateway registra error, usa valores por defecto o variables de entorno como fallback, inicia en modo degradado.
8. **El bucket de rate limit de un usuario específico se agota**: Requests subsiguientes de ese usuario reciben 429 con `retry-after`. Otros usuarios no se ven afectados.
9. **Un worker de negocio (SII) devuelve HTML en lugar de JSON** (común en APIs del gobierno chileno): El worker parsea el HTML, extrae datos relevantes, devuelve JSON estructurado.
10. **La sesión del worker de iConstruye expira durante una operación**: El worker detecta 401, solicita nuevo token vía credenciales configuradas, reintenta la operación una vez de forma transparente.
11. **Cambio incompatible en el trait Worker**: Una versión menor del core del gateway actualiza el trait. Todos los crates de worker deben recompilarse y probarse. Los crates fijan versión específica del core del gateway vía Cargo.toml.
12. **Alto uso de memoria bajo carga de streaming**: El gateway aplica backpressure — si hay demasiados streams activos, nuevos requests de streaming se encolan con 503 si la cola está llena.
13. **El circuit breaker se abre para un worker**: Requests que especifican ese proveedor explícitamente reciben error inmediato. Requests sin proveedor especificado se enrutan al siguiente worker disponible. La prueba half-open ocurre después del cooldown + jitter.
14. **Estampida en refresh de JWKS**: El gateway aplica jitter aleatorio (±25%) al TTL de caché de JWKS. Múltiples instancias del gateway refrescarán en momentos distintos, distribuyendo la carga en Thalamus.
15. **Request duplicado por fallo de UI**: Requests con idéntico (usuario, service_type, acción, hash de payload) que llegan dentro de 300ms se deduplican automáticamente. El segundo llamador recibe la misma respuesta que el primero.
16. **Request de worker de negocio falla después de todos los reintentos + failover**: El request se persiste en la Dead Letter Queue con el payload completo. Reintentos automáticos continúan con backoff exponencial (1m→2m→4m→8m→16m). Después de 5 reintentos fallidos, la entrada se marca como failed_permanent para revisión manual.
17. **Colisión de Idempotency-Key**: Dos requests diferentes con la misma clave se tratan como duplicados — el segundo recibe la respuesta cacheada del primero. Los clientes deben generar claves únicas por cada operación única.
18. **Terminación de pod con requests de streaming activos**: El gateway recibe SIGTERM, deja de aceptar nuevas conexiones, espera hasta shutdown_timeout_secs para que los streams emitan su `event: done` final, luego hace flush del audit log y sale.
19. **Todos los workers de negocio saturados (bulkhead por worker a capacidad)**: El gateway devuelve 503 inmediatamente en lugar de encolar indefinidamente. La cola de backpressure (si está habilitada) absorbe el exceso hasta su límite configurado.
20. **Arranque en frío del gateway con workers de calentamiento**: El endpoint de salud devuelve 503 ("starting") hasta que todos los workers de calentamiento pasen sus health checks o warmup_timeout_secs expire. El balanceador de carga solo enruta tráfico después de que el endpoint de salud devuelva 200.
21. **La caché de respuesta devuelve datos desactualizados para consulta de RUT del SII**: El TTL de 1 hora es aceptable para datos de registro tributario que cambian en escala de años. Para operaciones sensibles al tiempo, el TTL se configura más bajo o la caché se deshabilita.
