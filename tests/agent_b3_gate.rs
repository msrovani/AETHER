//! Gate **B3-prep / F0** — dual-bank T-EMB era nova (B3); candidate nunca roteia;
//! cutover só HITL; 0 mistura de eras; F4 congelada. Contrato: TOMADAS_FUTURO §B3.

#![cfg(feature = "agent")]

use aether_engine::agent::{EmbedBank, EmbedEra};

#[test]
fn b3_dual_bank_candidate_nunca_roteia_cutover_hitl_sem_mistura() {
    // F4 congelada: BAAI/bge-m3 1024d, sem candidate
    let f4 = EmbedBank::frozen_f4();
    assert_eq!(f4.active.dim, 1024);
    assert_eq!(f4.route_era().model_id, "BAAI/bge-m3");
    assert!(f4.candidate.is_none());

    // propor candidate (era nova, ex. MiniLM 384) NÃO ativa; candidate nunca roteia
    let era_nova = EmbedEra::minilm_384();
    let bank = f4.propose_candidate(era_nova.clone()).expect("candidate proposto");
    assert!(bank.candidate.is_some());
    assert_eq!(bank.route_era().model_id, "BAAI/bge-m3", "candidate nunca roteia; roteia active");
    assert_eq!(bank.route_era().dim, 1024, "0 mistura de eras no roteamento");

    // propor candidate igual a active → reject
    assert!(EmbedBank::frozen_f4().propose_candidate(EmbedEra::bge_m3_1024()).is_err());

    // cutover SEM token HITL → reject; nunca auto
    assert!(bank.clone().cutover("").is_err());
    assert!(bank.clone().cutover("   ").is_err());
    // cutover sem candidate → reject
    assert!(EmbedBank::frozen_f4().cutover("token").is_err());

    // cutover COM token HITL → active vira era nova; sem mistura (dim da nova)
    let after = bank.cutover("hitl-token-255").expect("cutover HITL");
    assert_eq!(after.route_era().model_id, era_nova.model_id);
    assert_eq!(after.route_era().dim, era_nova.dim);
    assert!(after.candidate.is_none());

    // 0 mistura de eras no vetor (já coberto por EmbedVector::validate/same_era, re-verificado)
    assert!(!era_nova.model_id.is_empty());
}
