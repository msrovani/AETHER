//! A3 — B-G0 (P2-lite): discriminação real de paráfrase com **labels
//! upstream** (PAWS-x), no espaço do donor e no nosso stack hv.
//!
//! Dados (gerados por `tools/b0_paws_harvest.py`, `tools/donor_map_embed.py`
//! e `tools/b0_pair_cosines.py`): `data/rounds/20261117_b0_paws/`
//! * `pair_cosines.json` — cos(s1,s2) por par + label (espaço do donor);
//! * `hv/bank.bin` + `hv/bank.idx` — o nosso bank (JL 768→10240 bits);
//! * `s2_query_hv.bin` — query por par (partner s2) projetada com o mesmo P.
//!
//! Métricas:
//! 1. **AUC** da similaridade separando parafraseia/não (espaço donor);
//! 2. **acurácia com banda de abstenção** no held-out (metade treino escolhe
//!    o limiar, metade teste mede; meio é abstain — cobertura declarada);
//! 3. **recall@1** do nosso stack (query s2 → rank-1 = fato do par).
//!
//! Sem os dados: skip silencioso (padrão sgdb_stdio). Artefato:
//! `target/b0_parafra_gate.json`.

#![cfg(feature = "m1")]

use std::fs;
use std::path::Path;

const ROUND: &str = "data/rounds/20261117_b0_paws";

fn data_ready() -> bool {
    Path::new(&format!("{ROUND}/pair_cosines.json")).exists()
        && Path::new(&format!("{ROUND}/hv/bank.bin")).exists()
        && Path::new(&format!("{ROUND}/s2_query_hv.bin")).exists()
}

/// AUC por ranqueamento (Mann-Whitney U normalizado), O(n log n).
fn auc(cosines: &[f64], is_label1: &[bool]) -> f64 {
    let mut idx: Vec<usize> = (0..cosines.len()).collect();
    idx.sort_by(|a, b| cosines[*a].partial_cmp(&cosines[*b]).unwrap());
    let n1 = is_label1.iter().filter(|b| **b).count() as f64;
    let n0 = (is_label1.len() as f64) - n1;
    if n1 == 0.0 || n0 == 0.0 {
        return 0.5;
    }
    // soma dos postos médios dos positivos
    let mut sum_ranks_pos = 0.0f64;
    let mut i = 0usize;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && cosines[idx[j + 1]] == cosines[idx[i]] {
            j += 1;
        }
        let avg_rank = ((i + j + 2) as f64) / 2.0; // postos 1-based médios
        for k in i..=j {
            if is_label1[idx[k]] {
                sum_ranks_pos += avg_rank;
            }
        }
        i = j + 1;
    }
    (sum_ranks_pos - n1 * (n1 + 1.0) / 2.0) / (n1 * n0)
}

#[test]
fn b0_parafra_discriminacao_e_recall() {
    if !data_ready() {
        eprintln!("B-G0: dados ausentes em {ROUND} — gere com o pipeline do forum 0487");
        return;
    }

    // ---------- 1) espaço do donor: AUC + banda de abstenção ----------
    let raw = fs::read(format!("{ROUND}/pair_cosines.json")).expect("pair_cosines");
    let text = String::from_utf8(raw).expect("utf8");
    let parsed = json_parse(&text);
    let cosines: Vec<f64> = parsed
        .iter()
        .filter_map(|line| {
            let key = "\"cos\":";
            let start = line.find(key)? + key.len();
            let end = line[start..].find(',')? + start;
            line[start..end].trim().parse().ok()
        })
        .collect();
    let is_label1: Vec<bool> = parsed
        .iter()
        .map(|line| line.contains("\"label\":\"parafraseia\""))
        .collect();
    assert_eq!(
        cosines.len(),
        is_label1.len(),
        "cosines/labels desalinhados"
    );
    let n = cosines.len();
    eprintln!("B-G0 n={n}");

    let auc_v = auc(&cosines, &is_label1);

    // Split determinístico (metades pares): treino escolhe limiar, teste mede.
    let mut train: Vec<usize> = (0..n).filter(|i| i % 2 == 0).collect();
    let test: Vec<usize> = (0..n).filter(|i| i % 2 == 1).collect();
    train.sort_by(|a, b| cosines[*a].partial_cmp(&cosines[*b]).unwrap());
    // limiar: média dos quartis centrais do treino
    let q1 = cosines[train[train.len() / 4]];
    let q3 = cosines[train[train.len() * 3 / 4]];
    let t_mid = (q1 + q3) / 2.0;
    let band = (q3 - q1) / 4.0;
    let (t_lo, t_hi) = (t_mid - band, t_mid + band);

    let mut covered = 0usize;
    let mut correct = 0usize;
    for &i in &test {
        let predict_label1 = cosines[i] >= t_mid;
        let truth = is_label1[i];
        if cosines[i] > t_hi || cosines[i] < t_lo {
            covered += 1;
            if predict_label1 == truth {
                correct += 1;
            }
        }
        // fora da banda = abstain (não conta nem para certo nem para errado)
    }
    let coverage = covered as f64 / test.len() as f64;
    let acc_covered = if covered > 0 {
        correct as f64 / covered as f64
    } else {
        0.0
    };
    eprintln!(
        "B-G0 donor: AUC={auc_v:.4} t={t_mid:.4} banda=[{t_lo:.4},{t_hi:.4}] cobertura={coverage:.3} acuracia_coberta={acc_covered:.3}"
    );
    assert!(
        auc_v >= 0.60,
        "AUC {auc_v:.3} < 0.60: o donor não separa pares PAWS — resultado honesto para o mapa (não baixar teto)"
    );
    assert!(
        acc_covered >= 0.70 && coverage >= 0.50,
        "acuracia_coberta={acc_covered:.3} (alvo 0.70) cobertura={coverage:.3} (alvo 0.50) — banda de abstenção mal calibrada"
    );

    // ---------- 2) nosso stack hv: recall@1 do partner ----------
    let bank = aether_engine::core::hv::HvBank::load(
        Path::new(&format!("{ROUND}/hv/bank.bin")),
        Path::new(&format!("{ROUND}/hv/bank.idx")),
    )
    .expect("bank");
    let qraw = fs::read(format!("{ROUND}/s2_query_hv.bin")).expect("s2_query_hv");
    let words = bank.words;
    assert_eq!(
        qraw.len(),
        bank.count() * words * 8,
        "queries/bank desalinhados"
    );

    let mut recall1 = 0usize;
    for q in 0..bank.count() {
        let query: Vec<u64> = qraw[q * words * 8..(q + 1) * words * 8]
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let mut best: (u32, u128) = (u32::MAX, 0);
        for i in 0..bank.count() {
            let d = aether_engine::core::hv::HvBank::hamming(&query, bank.atom_words(i));
            if d < best.0 {
                best = (d, bank.keys[i]);
            }
        }
        if best.1 == bank.keys[q] {
            recall1 += 1;
        }
    }
    let r1 = recall1 as f64 / bank.count() as f64;
    eprintln!("B-G0 stack: recall@1={r1:.4} ({recall1}/{})", bank.count());
    assert!(
        r1 >= 0.90,
        "recall@1 do stack {r1:.3} < 0.90 — a fiacao JL/bank não guarda o par"
    );

    let json = format!(
        "{{\"n\":{n},\"auc\":{auc_v:.4},\"t_mid\":{t_mid:.4},\"coverage\":{coverage:.4},\"acc_covered\":{acc_covered:.4},\"recall1_stack\":{r1:.4},\"created\":\"2026-11-18\"}}\n"
    );
    let _ = std::fs::create_dir_all("target");
    std::fs::write("target/b0_parafra_gate.json", json).expect("artefato");
}

/// JSON mínimo por linhas (o arquivo é um objeto de arrays simples).
fn json_parse(text: &str) -> Vec<String> {
    // cosines e labels são arrays paralelos; extraio pares pela ordem.
    let start = text.find("\"cosines\":").expect("cosines") + "\"cosines\":".len();
    let cos_part = &text[start..text[start..].find(']').expect("fech") + start];
    let cstart = text.find("\"labels\":").expect("labels") + "\"labels\":".len();
    let lab_part = &text[cstart..text[cstart..].find(']').expect("fech2") + cstart];
    let cosines: Vec<f64> = cos_part
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    let labels: Vec<String> = lab_part
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|x| x.trim().trim_matches('"').to_string())
        .collect();
    cosines
        .into_iter()
        .zip(labels)
        .map(|(c, l)| format!("\"cos\":{c},\"label\":\"{l}\""))
        .collect()
}
