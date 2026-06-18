use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
   use std::collections::HashMap;

   /// Request unificado que llega al gateway.
   /// Cualquier worker, sin importar su tipo, recibe esto.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ServiceRequest {
       /// Acción específica: "chat", "consultar_rut", "buscar_proveedores", etc.
       pub action: String,

       /// Body del request - el worker interpreta según la acción.
       pub payload: serde_json::Value,

       /// Parámetros opcionales: model, temperature, max_results, etc.
       pub params: HashMap<String, String>,

       /// Identidad del usuario desde el JWT de Thalamus.
       pub user_context: UserContext,
   }

   /// Identidad del usuario extraída del JWT.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct UserContext {
       pub sub: String,
       pub org_id: Option<String>,
       pub scopes: Vec<String>,
       pub oauth_tokens: HashMap<String, OAuthToken>,
   }

   /// Token OAuth que el usuario tiene para un proveedor externo.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct OAuthToken {
       pub access_token: String,
       pub refresh_token: Option<String>,
       pub expires_at: Option<i64>,
   }

   /// Respuesta unificada que devuelve cualquier worker.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ServiceResponse {
       pub data: serde_json::Value,
       pub worker: String,
       pub model: Option<String>,
       pub usage: Option<UsageInfo>,
   }

   /// Token usage (solo workers LLM llenan esto).
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct UsageInfo {
       pub prompt_tokens: u64,
       pub completion_tokens: u64,
       pub total_tokens: u64,
   }

   /// Un chunk en un stream de respuesta.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct StreamChunk {
       pub content: String,
       pub finish_reason: Option<String>,
   }

   // ── Type aliases para simplificar firmas ──

   pub type ChunkResult = Result<StreamChunk, crate::worker::error::WorkerError>;

   pub type ChunkStream = BoxStream<'static, ChunkResult>;

   pub type StreamResult = Result<ChunkStream, crate::worker::error::WorkerError>;
