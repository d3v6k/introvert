use introvert::embedding::{cosine_similarity, EmbeddingEngine, ACTION_PHRASES};
use std::path::Path;

#[test]
fn test_cosine_similarity_identical() {
    assert!((cosine_similarity(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]) - 1.0).abs() < 0.001);
}

#[test]
fn test_cosine_similarity_orthogonal() {
    assert!((cosine_similarity(&[1.0, 0.0], &[0.0, 1.0])).abs() < 0.001);
}

#[test]
fn test_cosine_similarity_opposite() {
    assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) - (-1.0)).abs() < 0.001);
}

#[test]
fn test_cosine_similarity_empty() {
    assert!((cosine_similarity(&[], &[])).abs() < 0.001);
}

#[test]
fn test_cosine_similarity_different_lengths() {
    assert!((cosine_similarity(&[1.0, 2.0], &[1.0, 2.0, 3.0])).abs() < 0.001);
}

#[test]
fn test_action_phrases_completeness() {
    assert_eq!(ACTION_PHRASES.len(), 12);
    let ids: Vec<&str> = ACTION_PHRASES.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&"battery_throttle"));
    assert!(ids.contains(&"db_prune"));
    assert!(ids.contains(&"media_cleanup"));
    assert!(ids.contains(&"connection_optimize"));
    assert!(ids.contains(&"message_batch"));
    assert!(ids.contains(&"prefetch"));
    assert!(ids.contains(&"sync_priority"));
    assert!(ids.contains(&"dedup"));
    assert!(ids.contains(&"health_score"));
    assert!(ids.contains(&"storage_quota"));
    assert!(ids.contains(&"adaptive_chunk"));
    assert!(ids.contains(&"tick"));
}

#[test]
fn test_embedding_engine_immediately_ready() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-ready"));
    engine.initialize();
    assert!(engine.is_ready(), "Engine should be immediately ready with keyword matching");
}

#[test]
fn test_intent_battery_throttle() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-1"));
    engine.initialize();
    let (id, score) = engine.match_intent("save battery please").expect("Should match");
    assert_eq!(id, "battery_throttle");
    assert!(score > 0.2, "Score: {}", score);
}

#[test]
fn test_intent_db_prune() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-2"));
    engine.initialize();
    let (id, _) = engine.match_intent("clean up the database").expect("Should match");
    assert_eq!(id, "db_prune");
}

#[test]
fn test_intent_storage_cleanup() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-3"));
    engine.initialize();
    let (id, _) = engine.match_intent("storage is full clean it").expect("Should match");
    assert!(id == "media_cleanup" || id == "storage_quota", "Got {}", id);
}

#[test]
fn test_intent_prefetch() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-4"));
    engine.initialize();
    let (id, _) = engine.match_intent("predict and prefetch files").expect("Should match");
    assert_eq!(id, "prefetch");
}

#[test]
fn test_intent_message_batch() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-5"));
    engine.initialize();
    let (id, _) = engine.match_intent("batch my messages queue them").expect("Should match");
    assert_eq!(id, "message_batch");
}

#[test]
fn test_intent_connection_optimize() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-6"));
    engine.initialize();
    let (id, _) = engine.match_intent("optimize connection speed").expect("Should match");
    assert_eq!(id, "connection_optimize");
}

#[test]
fn test_intent_dedup() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-7"));
    engine.initialize();
    let result = engine.match_intent("remove duplicates messages");
    if let Some((id, _)) = result {
        assert!(id == "dedup" || id == "db_prune", "Got {}", id);
    }
}

#[test]
fn test_intent_no_match_greeting() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-8"));
    engine.initialize();
    let result = engine.match_intent("hello how are you today");
    if let Some((_, score)) = result {
        assert!(score < 0.6, "Generic greeting should not match actions, score: {}", score);
    }
}

#[test]
fn test_intent_chunk_sizing() {
    let engine = EmbeddingEngine::new(Path::new("/tmp/test-embed-9"));
    engine.initialize();
    let result = engine.match_intent("adjust transfer chunk size");
    if let Some((id, _)) = result {
        assert!(id == "adaptive_chunk" || id == "message_batch", "Got {}", id);
    }
}
