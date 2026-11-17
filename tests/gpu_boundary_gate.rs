//! Fronteira da GPU — invariantes **estruturais** (G2/G3 do `docs/GPU_PLAN.md`).
//!
//! O ganho de GPU vale para lote/offline. O laço de 60 Hz, o `record` do F1, o
//! M5 e o solver do E2 **não** podem depender do backend gráfico. Este teste
//! garante isso lendo o fonte dos módulos quentes: se alguém importar
//! `core::gpu` neles, o teste falha — em qualquer combinação de features.
//!
//! Também cobre o contrato do backend: sem a feature `gpu`, `status()` responde
//! `NoFeature` (CPU-only completo), e `force_off` responde `Disabled`.

/// Módulos que **nunca** podem falar com a GPU (hot path e memória de fase).
const HOT: &[(&str, &str)] = &[
    (
        "simulation/closed_loop_organism.rs",
        include_str!("../src/simulation/closed_loop_organism.rs"),
    ),
    ("phase_memory.rs", include_str!("../src/phase_memory.rs")),
    (
        "core/knowledge_bridge_f1.rs",
        include_str!("../src/core/knowledge_bridge_f1.rs"),
    ),
    (
        "core/logic_pipeline.rs",
        include_str!("../src/core/logic_pipeline.rs"),
    ),
    (
        "core/thermodynamic_solver.rs",
        include_str!("../src/core/thermodynamic_solver.rs"),
    ),
];

#[test]
fn hot_path_nao_referencia_o_backend_gpu() {
    for (name, src) in HOT {
        for needle in ["core::gpu", "crate::core::gpu", "wgpu::"] {
            assert!(
                !src.contains(needle),
                "{name} referencia `{needle}` — GPU não entra no caminho quente (GPU_PLAN)"
            );
        }
    }
}

#[test]
fn crate_sem_feature_gpu_e_cpu_only_completo() {
    // `gpu_gate` (decisão) pode existir sempre; o backend só com a feature.
    assert!(
        !std::any::type_name::<aether_engine::core::gpu_gate::Workload>().is_empty(),
        "gpu_gate (decisão) precisa existir sem a feature"
    );

    #[cfg(not(feature = "gpu"))]
    {
        let st = aether_engine::core::gpu::status();
        assert!(!st.available, "sem a feature não pode haver device");
        assert_eq!(
            st.off,
            Some(aether_engine::core::gpu::GpuOff::NoFeature),
            "motivo deveria ser NoFeature (CPU-only completo)"
        );
        let off = aether_engine::core::gpu::status_with(true);
        assert_eq!(off.off, Some(aether_engine::core::gpu::GpuOff::Disabled));
    }

    #[cfg(feature = "gpu")]
    {
        // Com a feature, o desligamento explícito continua obrigatório.
        let off = aether_engine::core::gpu::status_with(true);
        assert!(!off.available);
        assert_eq!(off.off, Some(aether_engine::core::gpu::GpuOff::Disabled));
    }
}

#[test]
fn custo_de_admissao_esta_documentado_no_gate() {
    // As quatro condições de `docs/GPU_PLAN.md` usam estas constantes; elas não
    // podem mudar sem atualizar o plano.
    use aether_engine::core::gpu_gate;
    assert_eq!(gpu_gate::MIN_GPU_N, 512);
    assert!((gpu_gate::CPU_BANDWIDTH_GBPS - 50.0).abs() < f64::EPSILON);
    assert!((gpu_gate::GPU_BANDWIDTH_GBPS - 300.0).abs() < f64::EPSILON);
    assert!((gpu_gate::BYTES_PER_EDGE - 12.0).abs() < f64::EPSILON);
}
