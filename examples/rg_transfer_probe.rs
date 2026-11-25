//! E5 — RG spine: assinaturas macro cross-domínio (Sprint A, R6/F23).
//!
//! Constrói dois `RenormalizationHierarchy` com assinaturas macro próximas
//! (jitter pequeno), imprime fase/ordem/assinatura e a similaridade de Hamming
//! via `SelfField::macro_signature_similarity` — o consumidor prometido p/
//! transferência zero-shot (G-ISO).

use aether_engine::core::renormalization::RenormalizationHierarchy;
use aether_engine::core::self_field::SelfField;

fn hierarchy(tag: f32, shift: f32) -> RenormalizationHierarchy {
    let mut rg = RenormalizationHierarchy::new(64).expect("rg64");
    let mut f = vec![0.0f32; 64];
    for i in 0..64 {
        f[i] = 0.5 + 0.3 * tag * ((i % 7) as f32).sin() + shift;
    }
    rg.forward(&f).expect("forward");
    rg
}

fn main() {
    // Mesma dinâmica macro (shift ≈ 0) => assinaturas próximas (G-ISO).
    let a = hierarchy(0.01, 0.0);
    let b = hierarchy(0.01, 0.001);
    let sa = a.macro_signature();
    let sb = b.macro_signature();
    let sim = SelfField::macro_signature_similarity(&sa, &sb);
    println!("A: phase={:.4} order={:.4} sig={:02x?}", a.macro_phase(), a.macro_order(), sa);
    println!("B: phase={:.4} order={:.4} sig={:02x?}", b.macro_phase(), b.macro_order(), sb);
    println!("similaridade = {sim:.3} (próxima assinatura: macro mapeada).");
}
