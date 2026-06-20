//! Local Embedding Engine — Computes sentence embeddings using a BERT model
//! (all-MiniLM-L6-v2, ~23MB). Runs inference on a dedicated low-priority thread
//! to never block the UI. Falls back to keyword matching when the model isn't loaded.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, debug};

use candle_core::{Device, Tensor, DType};
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use candle_nn::VarBuilder;
use tokenizers::Tokenizer;

// ─── Action Intent Definitions ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionIntent {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentMapping {
    pub version: u32,
    pub actions: Vec<ActionIntent>,
}

// ─── Cosine Similarity ──────────────────────────────────────────────────────

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ─── Action Phrases ─────────────────────────────────────────────────────────

pub const ACTION_PHRASES: &[(&str, &str)] = &[
    ("battery_throttle", "slow down background activity save battery reduce power consumption when battery is low"),
    ("db_prune", "clean up database remove old expired sessions cache garbage collection optimize database"),
    ("media_cleanup", "clean up storage remove orphaned files free disk space storage full"),
    ("connection_optimize", "optimize connection improve network speed find better peers upgrade direct connection"),
    ("message_batch", "batch messages queue messages hold messages send later poor connectivity"),
    ("prefetch", "prefetch files predict downloads fetch files proactively anticipate file requests"),
    ("sync_priority", "prioritize sync sync important contacts first sync unread messages urgent sync"),
    ("dedup", "remove duplicates deduplicate messages prevent duplicate downloads remove repeated content"),
    ("health_score", "connection health check peer reliability connection quality score"),
    ("storage_quota", "storage quota disk usage check storage limit storage full clean up storage"),
    ("adaptive_chunk", "chunk size adjust transfer speed optimize file transfer chunks adjust transfer size"),
    ("tick", "run maintenance perform health check run all maintenance tasks engine status"),
];

// ─── BERT Inference State ───────────────────────────────────────────────────

struct BertInference {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    #[allow(dead_code)]
    config: BertConfig,
}

// ─── Embedding Engine ───────────────────────────────────────────────────────

pub struct EmbeddingEngine {
    model_dir: PathBuf,
    inference: Arc<RwLock<Option<BertInference>>>,
    intent_mapping: Arc<RwLock<Option<IntentMapping>>>,
    is_ready: Arc<RwLock<bool>>,
    is_loading: Arc<RwLock<bool>>,
}

impl EmbeddingEngine {
    pub fn new(cache_dir: &Path) -> Self {
        let model_dir = cache_dir.join("intro-claw-models");
        Self {
            model_dir,
            inference: Arc::new(RwLock::new(None)),
            intent_mapping: Arc::new(RwLock::new(None)),
            is_ready: Arc::new(RwLock::new(false)),
            is_loading: Arc::new(RwLock::new(false)),
        }
    }

    pub fn is_ready(&self) -> bool {
        *self.is_ready.read()
    }

    pub fn is_loading(&self) -> bool {
        *self.is_loading.read()
    }

    /// Initialize keyword matching immediately, then load BERT model in background.
    pub fn initialize(&self) {
        if *self.is_ready.read() || *self.is_loading.read() {
            return;
        }

        // Immediately populate keyword-based intents
        let mapping = Self::build_default_intent_mapping();
        *self.intent_mapping.write() = Some(mapping);
        *self.is_ready.write() = true;

        // Background: download + load BERT model for vector similarity
        *self.is_loading.write() = true;
        let inference = self.inference.clone();
        let intent_mapping = self.intent_mapping.clone();
        let model_dir = self.model_dir.clone();
        let is_loading = self.is_loading.clone();

        std::thread::Builder::new()
            .name("embedding-init".to_string())
            .spawn(move || {
                // Set low thread priority (nice 10) to avoid blocking UI
                #[cfg(unix)]
                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, 0, 10);
                }

                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async {
                    match Self::load_model(&model_dir).await {
                        Ok(bert_inference) => {
                            info!("[Embedding] BERT model loaded — vector similarity active");
                            *inference.write() = Some(bert_inference);

                            // Pre-compute action embeddings
                            let inf = inference.read();
                            if let Some(ref inf) = *inf {
                                let mut mapping = intent_mapping.write();
                                if let Some(ref mut m) = *mapping {
                                    for action in &mut m.actions {
                                        if action.embedding.is_none() {
                                            action.embedding = inf.encode(&action.description).ok();
                                        }
                                    }
                                    info!("[Embedding] Pre-computed {} action embeddings", m.actions.len());
                                }
                            }
                        }
                        Err(e) => {
                            debug!("[Embedding] BERT model unavailable: {} — keyword matching only", e);
                        }
                    }
                    *is_loading.write() = false;
                });
            })
            .ok();
    }

    async fn load_model(model_dir: &Path) -> Result<BertInference, String> {
        tokio::fs::create_dir_all(model_dir)
            .await
            .map_err(|e| format!("Failed to create model dir: {}", e))?;

        let tokenizer_path = model_dir.join("tokenizer.json");
        let weights_path = model_dir.join("model.safetensors");
        let config_path = model_dir.join("config.json");

        // Download if not cached
        if !tokenizer_path.exists() || !weights_path.exists() || !config_path.exists() {
            info!("[Embedding] Downloading all-MiniLM-L6-v2 model (~23MB)...");
            let api = hf_hub::api::sync::Api::new()
                .map_err(|e| format!("HF Hub init failed: {}", e))?;
            let repo = api.model("sentence-transformers/all-MiniLM-L6-v2".to_string());

            for filename in &["tokenizer.json", "model.safetensors", "config.json"] {
                let file = repo.get(filename)
                    .map_err(|e| format!("Failed to download {}: {}", filename, e))?;
                let dest = model_dir.join(filename);
                std::fs::copy(&file, &dest)
                    .map_err(|e| format!("Failed to copy {}: {}", filename, e))?;
            }
            info!("[Embedding] Model downloaded to {:?}", model_dir);
        }

        // Load on CPU
        let device = Device::Cpu;

        // Load config
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let config: BertConfig = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse config: {}", e))?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        // Load model weights via safetensors
        let weights = std::fs::read(&weights_path)
            .map_err(|e| format!("Failed to read weights: {}", e))?;

        let vb = VarBuilder::from_slice_safetensors(&weights, DType::F32, &device)
            .map_err(|e| format!("Failed to create VarBuilder: {}", e))?;

        let model = BertModel::load(vb, &config)
            .map_err(|e| format!("Failed to load BERT model: {}", e))?;

        info!("[Embedding] BERT model initialized on CPU ({} layers, hidden={})",
            config.num_hidden_layers, config.hidden_size);

        Ok(BertInference { model, tokenizer, device, config })
    }

    fn build_default_intent_mapping() -> IntentMapping {
        let actions: Vec<ActionIntent> = ACTION_PHRASES
            .iter()
            .map(|(id, desc)| ActionIntent {
                id: id.to_string(),
                description: desc.to_string(),
                embedding: None,
            })
            .collect();
        IntentMapping { version: 1, actions }
    }

    /// Keyword-based intent matching (fast, no model needed).
    pub fn match_intent(&self, query: &str) -> Option<(String, f32)> {
        let intents = self.intent_mapping.read();
        let intents = intents.as_ref()?;
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .collect();

        if query_words.is_empty() {
            return None;
        }

        let mut best_match: Option<(String, f32)> = None;
        let mut best_score: f32 = 0.0;

        for action in &intents.actions {
            let desc_lower = action.description.to_lowercase();
            let desc_words: Vec<&str> = desc_lower.split_whitespace().collect();
            let total_desc_words = desc_words.len() as f32;
            if total_desc_words == 0.0 { continue; }

            let mut matched_words = 0.0_f32;
            let mut exact_matches = 0.0_f32;

            for qw in &query_words {
                for dw in &desc_words {
                    if *dw == *qw {
                        exact_matches += 1.0;
                        matched_words += 1.0;
                    } else if dw.contains(qw) || qw.contains(dw) {
                        matched_words += 0.5;
                    }
                }
            }

            let coverage = matched_words / query_words.len() as f32;
            let density = matched_words / total_desc_words;
            let exact_bonus = exact_matches / query_words.len() as f32;
            let score = coverage * 0.6 + density * 0.2 + exact_bonus * 0.2;

            if score > best_score && score > 0.2 {
                best_score = score;
                best_match = Some((action.id.clone(), score));
            }
        }

        best_match
    }

    /// Full semantic match: keyword first, then BERT cosine similarity.
    pub fn process_query(&self, query: &str) -> Option<(String, f32)> {
        if !self.is_ready() {
            return None;
        }

        // Phase 1: Fast keyword matching
        if let Some(result) = self.match_intent(query) {
            if result.1 >= 0.75 {
                return Some(result);
            }
        }

        // Phase 2: BERT embedding similarity
        let intents = self.intent_mapping.read();
        if let Some(intents) = intents.as_ref() {
            let inference = self.inference.read();
            if let Some(ref inf) = *inference {
                if let Ok(query_emb) = inf.encode(query) {
                    let mut best_score = 0.0_f32;
                    let mut best_id = String::new();
                    for action in &intents.actions {
                        if let Some(ref emb) = action.embedding {
                            let score = cosine_similarity(&query_emb, emb);
                            if score > best_score {
                                best_score = score;
                                best_id = action.id.clone();
                            }
                        }
                    }
                    if best_score >= 0.75 && best_id.is_empty() == false {
                        return Some((best_id, best_score));
                    }
                }
            }
        }

        // Phase 3: Return best keyword match even below threshold
        self.match_intent(query)
    }
}

impl BertInference {
    /// Tokenize text, run BERT forward pass, mean-pool, L2-normalize → 384-dim vector.
    pub fn encode(&self, text: &str) -> Result<Vec<f32>, String> {
        let encoding = self.tokenizer.encode(text, true)
            .map_err(|e| format!("Tokenization failed: {}", e))?;

        let input_ids: Vec<u32> = encoding.get_ids().iter().copied().collect();
        let attention_mask: Vec<u32> = encoding.get_attention_mask().iter().copied().collect();

        let seq_len = input_ids.len();
        let input_ids_tensor = Tensor::new(&input_ids[..], &self.device)
            .map_err(|e| format!("Tensor creation failed: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {}", e))?;

        let token_type_ids = vec![0u32; seq_len];
        let token_type_ids_tensor = Tensor::new(&token_type_ids[..], &self.device)
            .map_err(|e| format!("Tensor creation failed: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {}", e))?;

        let attention_mask_tensor = Tensor::new(&attention_mask[..], &self.device)
            .map_err(|e| format!("Tensor creation failed: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {}", e))?;

        // BERT forward pass → [batch, seq_len, hidden_size]
        let output = self.model.forward(
            &input_ids_tensor,
            &token_type_ids_tensor,
            Some(&attention_mask_tensor),
        ).map_err(|e| format!("BERT forward pass failed: {}", e))?;

        // Mean pooling: average non-padding token embeddings
        // output shape: [1, seq_len, 384]
        let mask_f32 = attention_mask_tensor
            .to_dtype(DType::F32)
            .map_err(|e| format!("Mask dtype failed: {}", e))?;

        // Expand mask to [1, seq_len, 1] for broadcasting
        let mask_expanded = mask_f32.unsqueeze(2)
            .map_err(|e| format!("Mask expand failed: {}", e))?;

        // Masked sum: output * mask
        let masked = output.broadcast_mul(&mask_expanded)
            .map_err(|e| format!("Masked multiply failed: {}", e))?;

        // Sum along seq_len dim (dim 1), then divide by sum of mask
        let sum = masked.sum(1)
            .map_err(|e| format!("Sum failed: {}", e))?;

        let mask_sum = mask_f32.sum(1)
            .map_err(|e| format!("Mask sum failed: {}", e))?
            .unsqueeze(2)
            .map_err(|e| format!("Mask sum unsqueeze failed: {}", e))?;

        let pooled = sum.broadcast_div(&mask_sum)
            .map_err(|e| format!("Pooled divide failed: {}", e))?;

        // L2 normalize
        let norm = pooled.sqr()
            .map_err(|e| format!("Sqr failed: {}", e))?
            .sum_all()
            .map_err(|e| format!("Sum all failed: {}", e))?
            .sqrt()
            .map_err(|e| format!("Sqrt failed: {}", e))?;

        let eps = Tensor::new(1e-12f32, &self.device).unwrap();
        let normalized = pooled.broadcast_div(&(norm.broadcast_add(&eps).unwrap())).unwrap();

        // Extract to Vec<f32>
        let embedding = normalized
            .squeeze(0)
            .map_err(|e| format!("Squeeze failed: {}", e))?
            .to_vec1::<f32>()
            .map_err(|e| format!("to_vec1 failed: {}", e))?;

        Ok(embedding)
    }
}
