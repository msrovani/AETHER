//! Stress determinístico do undo transacional (modelo vs backend real).
//!
//! Cada iteração simula uma tool que escreve e registra o efeito, e de vez em
//! quando faz rewind para uma marca aleatória. O estado final do backend tem
//! que bater com a reaplicação do log até a marca — sem "fantasmas".

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aether_engine::agent::{
    Effect, SgdbUndoBackend, Snapshot, ToolContext, UndoBackend, AUTHORITY_QUARANTINE,
};
use aether_engine::core::knowledge_bridge_f1::ScopeId;

fn temp_journal(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "aether_undo_stress_{tag}_{}_{nanos}.jsonl",
        std::process::id()
    ))
}

/// LCG determinístico (xorshift-ish), sem dependências.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// Estado esperado: reaplica o log de escritas `[0..mark)`.
fn model(log: &[(String, String)], keys: &[&str]) -> Vec<Option<String>> {
    let mut state: Vec<Option<String>> = vec![None; keys.len()];
    for (key, value) in log {
        if let Some(index) = keys.iter().position(|candidate| candidate == key) {
            state[index] = Some(value.clone());
        }
    }
    state
}

#[test]
fn gate_stress_undo_bate_com_modelo() {
    let path = temp_journal("model");
    let backend = std::sync::Arc::new(SgdbUndoBackend::new(&path));
    let budget = aether_engine::agent::Budget {
        max_steps: 32,
        max_ms: 60_000,
        max_tokens: 0,
    };
    let mut ctx =
        ToolContext::new(ScopeId::new(1, 0, 0), budget, 99).with_undo_backend(backend.clone());

    let keys = ["a", "b", "c"];
    let mut log: Vec<(String, String)> = Vec::new();
    let mut rng = Rng::new(0xA37E_2026);

    for step in 0..240u32 {
        let key = keys[rng.below(keys.len())].to_string();
        let value = format!("v{step}");
        let prev: Option<Snapshot> = backend
            .write(&key, &value, AUTHORITY_QUARANTINE)
            .expect("write");
        ctx.push_effect(Effect::MemoryWrite {
            key: key.clone(),
            prev,
        });
        log.push((key, value));

        // 35% das iterações desfazem até uma marca aleatória.
        if rng.below(100) < 35 {
            let mark = rng.below(log.len() + 1);
            let report = ctx.rewind_report(mark);
            assert!(
                report.is_ok(),
                "rewind falhou na iteração {step}: {}",
                report.summary()
            );
            log.truncate(mark);
        }

        let expected = model(&log, &keys);
        for (index, key) in keys.iter().enumerate() {
            let observed = backend.get(key).map(|snapshot| snapshot.text);
            assert_eq!(
                observed,
                expected[index],
                "divergência na iteração {step}, chave {key} (log={})",
                log.len()
            );
        }
    }

    // No fim, desfaz tudo: store tem que ficar vazio.
    let report = ctx.rewind_report(0);
    assert!(report.is_ok(), "{}", report.summary());
    for key in keys {
        assert!(
            backend.get(key).is_none(),
            "sobrou `{key}` após rewind total"
        );
    }
    let _ = std::fs::remove_file(&path);
}
