use introvert::embedding::{cosine_similarity, EmbeddingEngine, ACTION_PHRASES};
use std::path::Path;

#[test]
fn embedding_benchmark_suite() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Intro-Claw Embedding Engine — Integration Benchmark Suite  ");
    println!("═══════════════════════════════════════════════════════════════\n");

    // ─── Test Queries ────────────────────────────────────────────────────
    let test_queries = vec![
        // Should match battery_throttle
        ("save battery please", "battery_throttle"),
        ("my battery is draining fast slow things down", "battery_throttle"),
        ("reduce power consumption", "battery_throttle"),

        // Should match db_prune
        ("clean up the database", "db_prune"),
        ("remove old expired sessions", "db_prune"),
        ("garbage collection", "db_prune"),

        // Should match media_cleanup / storage_quota
        ("storage is full clean it", "media_cleanup|storage_quota"),
        ("free disk space remove orphaned files", "media_cleanup"),

        // Should match message_batch
        ("batch my messages queue them", "message_batch"),
        ("hold messages send later connectivity bad", "message_batch"),

        // Should match prefetch
        ("predict and prefetch files", "prefetch"),
        ("proactively fetch files from contacts", "prefetch"),

        // Should match connection_optimize
        ("optimize my connection speed", "connection_optimize"),
        ("find better peers upgrade connection", "connection_optimize"),

        // Should match dedup
        ("remove duplicates messages", "dedup|db_prune"),
        ("stop duplicate downloads", "dedup"),

        // Should match sync_priority
        ("sync important contacts first", "sync_priority"),
        ("prioritize unread messages sync", "sync_priority"),

        // Should match adaptive_chunk
        ("adjust transfer chunk size", "adaptive_chunk|message_batch"),
        ("optimize file transfer chunks", "adaptive_chunk"),

        // Should match storage_quota
        ("check storage limit", "storage_quota"),
        ("disk usage too high", "storage_quota"),

        // Should match health_score
        ("check connection health", "health_score"),
        ("peer reliability score", "health_score"),

        // Should match tick
        ("run all maintenance tasks", "tick"),
        ("perform full health check", "tick"),

        // Should NOT match anything high
        ("hello how are you today", ""),
        ("what is the weather like", ""),
        ("tell me a joke", ""),
    ];

    // ─── Phase 1: Keyword Matching Benchmark ─────────────────────────────
    println!("─── PHASE 1: Keyword Matching Scores ─────────────────────────");
    println!("{:<50} {:<20} {:<8} {}", "Query", "Matched Action", "Score", "Pass?");
    println!("{}", "─".repeat(90));

    let engine = EmbeddingEngine::new(Path::new("/tmp/embed-bench"));
    engine.initialize();

    let mut keyword_correct = 0;
    let mut keyword_total = 0;

    for (query, expected) in &test_queries {
        let result = engine.match_intent(query);
        let (matched_id, score) = match result {
            Some((id, s)) => (id, s),
            None => ("(none)".to_string(), 0.0),
        };

        let expected_ids: Vec<&str> = if expected.is_empty() {
            vec![]
        } else {
            expected.split('|').collect()
        };

        let pass = if expected_ids.is_empty() {
            score < 0.5
        } else {
            expected_ids.iter().any(|e| *e == matched_id)
        };

        if pass {
            keyword_correct += 1;
        }
        keyword_total += 1;

        let status = if pass { "✅" } else { "❌" };
        println!("{:<50} {:<20} {:<8.3} {}", query, matched_id, score, status);
    }
    println!("{}", "─".repeat(90));
    println!("Keyword Accuracy: {}/{} ({:.0}%)\n", keyword_correct, keyword_total, keyword_correct as f64 / keyword_total as f64 * 100.0);

    // ─── Phase 2: Cosine Similarity Benchmark ────────────────────────────
    println!("─── PHASE 2: Cosine Similarity Scores ─────────────────────────");
    println!("{:<50} {:<20} {:<8} {}", "Query", "Best Match", "Score", "Pass?");
    println!("{}", "─".repeat(90));

    let mut sim_correct = 0;
    let mut sim_total = 0;

    for (query, expected) in &test_queries {
        let result = engine.process_query(query);
        let (matched_id, score) = match result {
            Some((id, s)) => (id, s),
            None => ("(none)".to_string(), 0.0),
        };

        let expected_ids: Vec<&str> = if expected.is_empty() {
            vec![]
        } else {
            expected.split('|').collect()
        };

        let pass = if expected_ids.is_empty() {
            score < 0.5
        } else {
            expected_ids.iter().any(|e| *e == matched_id)
        };

        if pass {
            sim_correct += 1;
        }
        sim_total += 1;

        let status = if pass { "✅" } else { "❌" };
        println!("{:<50} {:<20} {:<8.3} {}", query, matched_id, score, status);
    }
    println!("{}", "─".repeat(90));
    println!("Combined Accuracy: {}/{} ({:.0}%)\n", sim_correct, sim_total, sim_correct as f64 / sim_total as f64 * 100.0);

    // ─── Phase 3: Cosine Similarity Pairwise ─────────────────────────────
    println!("─── PHASE 3: Cosine Similarity Pairwise Distances ────────────");
    let a1 = vec![1.0, 0.0, 0.0];
    let a2 = vec![0.0, 1.0, 0.0];
    let a3 = vec![-1.0, 0.0, 0.0];
    let a4 = vec![0.707, 0.707, 0.0];

    println!("  identical  → cosine(a1, a1) = {:.4}", cosine_similarity(&a1, &a1));
    println!("  orthogonal → cosine(a1, a2) = {:.4}", cosine_similarity(&a1, &a2));
    println!("  opposite   → cosine(a1, a3) = {:.4}", cosine_similarity(&a1, &a3));
    println!("  45 degrees → cosine(a1, a4) = {:.4}", cosine_similarity(&a1, &a4));
    println!("  empty      → cosine([], []) = {:.4}", cosine_similarity(&[], &[]));

    // ─── Phase 4: Action Phrase Coverage ──────────────────────────────────
    println!("\n─── PHASE 4: Action Intent Coverage ──────────────────────────");
    for (id, desc) in ACTION_PHRASES {
        let result = engine.match_intent(desc);
        let (matched_id, score) = match result {
            Some((id, s)) => (id, s),
            None => ("(none)".to_string(), 0.0),
        };
        let pass = matched_id == *id;
        let status = if pass { "✅" } else { "❌" };
        println!("  {:<25} self-match: {} (score: {:.3}) {}", id, matched_id, score, status);
    }

    // ─── Phase 5: Performance ────────────────────────────────────────────
    println!("\n─── PHASE 5: Performance (keyword matching) ──────────────────");
    let queries = vec![
        "save battery", "clean database", "storage full", "batch messages",
        "prefetch files", "optimize connection", "remove duplicates",
        "sync contacts", "adjust chunks", "disk usage", "health check", "run maintenance",
    ];

    let start = std::time::Instant::now();
    let iterations = 100_000;
    for _ in 0..iterations {
        for q in &queries {
            engine.match_intent(q);
        }
    }
    let elapsed = start.elapsed();
    let per_query_us = elapsed.as_micros() as f64 / (iterations * queries.len()) as f64;
    println!("  {} iterations × {} queries = {} total matches", iterations, queries.len(), iterations * queries.len());
    println!("  Total time: {:?}", elapsed);
    println!("  Per query:  {:.1} µs ({:.0} queries/sec)", per_query_us, 1_000_000.0 / per_query_us);

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("  Benchmark complete.");
    println!("═══════════════════════════════════════════════════════════════");
}
