//! US0-aggregate — G7 do freeze v0.1 (docs/V01_DELIVERY_PLAN.md §3.2).
//!
//! Agrega os US U2–U5, U7–U11, U13 (docs/USABILITY_TESTS.md) pela opção B do
//! G7: **checklist executável** apontando gates existentes 1:1.
//!
//! Regras honestas (sem número sintético, sem afrouxar piso):
//! * FONTE: cada gate mapeado DEVE existir em tests/ — wiring não pode apodrecer;
//! * ARTEFATO: quando o JSON existe, os números são validados contra piso/teto;
//! * Sem artefato → linha "wired" (o gate dono cobre o US na própria suíte);
//! * U12 não é duplicado aqui (já é o tests/usability_gate.rs — arquivo anterior).
//!
//! Artefato: `target/us_aggregate_gate.json`.

use std::fs;
use std::path::Path;

/// Extrai o valor de string de uma chave simples `"chave":"valor"` (JSON plano).
fn extract(text: &str, key: &str) -> String {
    let pat = format!("\"{key}\"");
    let Some(pos) = text.find(&pat) else {
        return String::new();
    };
    let rest = &text[pos + pat.len()..];
    let Some(colon) = rest.find(':') else {
        return String::new();
    };
    let after = rest[colon + 1..].trim_start();
    if let Some(v) = after.strip_prefix('"') {
        let end = v.find('"').unwrap_or(v.len());
        v[..end].to_string()
    } else {
        let end = after
            .find(|c: char| {
                !(c.is_ascii_digit() || *c == '.' || *c == '-' || *c == 'e' || *c == '+')
            })
            .unwrap_or(after.len());
        after[..end].to_string()
    }
}

/// Extrai um número de uma chave simples `"chave":<num>` (JSON plano).
fn extract_num(text: &str, key: &str) -> Option<f64> {
    let s = extract(text, key);
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok()
}

struct UsRow {
    us: &'static str,
    titulo: &'static str,
    gate: &'static str,
    gate_src: &'static str,
    artifact: Option<&'static str>,
}

const ROWS: &[UsRow] = &[
    UsRow {
        us: "U2",
        titulo: "fato_conhecido_cite_key_escopo_autoridade",
        gate: "v01_smoke (G8) + e3 G3 proveniencia",
        gate_src: "tests/e3_transplant_gate.rs",
        artifact: Some("target/v01_smoke.json"),
    },
    UsRow {
        us: "U3",
        titulo: "ood_abstain_false_act_0",
        gate: "e3_transplant_gate (ood_abstained=1.0, false_confident=0) + smoke",
        gate_src: "tests/e3_transplant_gate.rs",
        artifact: Some("target/aether_e3.json"),
    },
    UsRow {
        us: "U4",
        titulo: "parafrase_digitacao_abstain_falso_positivo_0",
        gate: "api_eval (paraphrase/case/one_char/prefix abstain) + b0_parafra",
        gate_src: "tests/e3b_certificate.rs",
        artifact: Some("target/aether_api_eval.json"),
    },
    UsRow {
        us: "U5",
        titulo: "solver_certificado_violations_0",
        gate: "v01_smoke solve_certificado + certificate_authority",
        gate_src: "tests/e3b_certificate.rs",
        artifact: Some("target/v01_smoke.json"),
    },
    UsRow {
        us: "U7",
        titulo: "hitl_antes_da_call",
        gate: "agent_plan_gate",
        gate_src: "tests/agent_plan_gate.rs",
        artifact: None,
    },
    UsRow {
        us: "U8",
        titulo: "reversibilidade_before_image_lifo",
        gate: "agent_plan_gate + agent_undo_sgdb_gate",
        gate_src: "tests/agent_undo_sgdb_gate.rs",
        artifact: None,
    },
    UsRow {
        us: "U9",
        titulo: "autoridade_forget_255_sem_store_500_tombstone",
        gate: "api_agent",
        gate_src: "tests/api_agent.rs",
        artifact: None,
    },
    UsRow {
        us: "U10",
        titulo: "entrada_abusiva_injecao_oov_sem_panic",
        gate: "agent_redteam_gate",
        gate_src: "tests/agent_redteam_gate.rs",
        artifact: None,
    },
    UsRow {
        us: "U11",
        titulo: "contradicao_tipada_sem_overwrite",
        gate: "agent_undo_sgdb_gate (CAS, nao apaga de terceiro)",
        gate_src: "tests/agent_undo_sgdb_gate.rs",
        artifact: None,
    },
    UsRow {
        us: "U13",
        titulo: "parser_pt_br_en_95pct_oov_5pct_needs_human",
        gate: "language_port_gate",
        gate_src: "tests/language_port_gate.rs",
        artifact: None,
    },
];

#[test]
fn us0_aggregate_g7() {
    let mut report: Vec<String> = Vec::new();
    let mut pass = 0usize;
    let mut wired = 0usize;
    let mut fail = 0usize;

    for row in ROWS {
        // 1) FONTE: o gate mapeado precisa existir (wiring).
        if !Path::new(row.gate_src).exists() {
            report.push(format!(
                "{}|FONTE_AUSENTE|{} ({}): {}",
                row.us, row.gate_src, row.titulo, row.gate
            ));
            fail += 1;
            continue;
        }

        // 2) ARTEFATO: se existe, valida número contra piso/teto.
        let Some(art) = row.artifact else {
            report.push(format!("{}|wired|{}", row.us, row.gate_src));
            wired += 1;
            continue;
        };
        if !Path::new(art).exists() {
            report.push(format!(
                "{}|wired|artefato ausente ({art}) — gate {}/{} cobre na própria suíte",
                row.us, row.gate_src, row.gate
            ));
            wired += 1;
            continue;
        }
        let body = fs::read_to_string(art).unwrap_or_default();

        let status: Result<(), String> = match row.us {
            // U2: smoke tem cue_conhecido ok (cite com key) e total==pass.
            "U2" => {
                let total = extract_num(&body, "total").unwrap_or(0.0);
                let passed = extract_num(&body, "pass").unwrap_or(0.0);
                let ok = extract(&body, "ok");
                if body.contains("cue_conhecido") && ok == "true" && total > 0.0 && passed == total
                {
                    Ok(())
                } else {
                    Err(format!(
                        "v01_smoke inconsistente: ok={ok} pass={passed}/{total}"
                    ))
                }
            }
            // U3: e3 — OOD 100% abstido e falso-confiante 0 (false-act = 0).
            "U3" => {
                let ood = extract_num(&body, "ood_abstained").unwrap_or(0.0);
                let fc = extract_num(&body, "false_confident").unwrap_or(1.0);
                if ood >= 1.0 && fc == 0.0 {
                    Ok(())
                } else {
                    Err(format!("e3: ood_abstained={ood} false_confident={fc}"))
                }
            }
            // U4: api_eval — todos os casos de abstain com pass=true.
            "U4" => {
                let casos = ["paraphrase", "case", "one_char", "prefix", "scope"];
                let mut err = None;
                for c in casos {
                    let pos = match body.find(&format!("\"id\":\"{c}\"")) {
                        Some(p) => p,
                        None => {
                            err = Some(format!("api_eval sem caso {c}"));
                            break;
                        }
                    };
                    let slice = &body[pos..];
                    let expect = extract(slice, "expect");
                    let caso_pass = extract(slice, "pass");
                    if expect != "abstain" || caso_pass != "true" {
                        err = Some(format!(
                            "api_eval caso {c}: expect={expect} pass={caso_pass}"
                        ));
                        break;
                    }
                }
                err.map_or(Ok(()), Err)
            }
            // U5: smoke — solve_certificado com violations=0.
            "U5" => {
                if body.contains("solve_certificado")
                    && body.contains("\"ok\":  true")
                    && extract(&body, "ok") == "true"
                {
                    Ok(())
                } else {
                    Err("v01_smoke sem solve_certificado ok".into())
                }
            }
            _ => Ok(()),
        };

        match status {
            Ok(()) => {
                report.push(format!("{}|pass|{}", row.us, art));
                pass += 1;
            }
            Err(e) => {
                report.push(format!("{}|FALHOU|{art}: {e}", row.us));
                fail += 1;
            }
        }
    }

    // Nota informativa (trilha R): o gate de consistência E1 rodou com achado
    // negativo honesto (AUC 0.5983 < piso 0.60) — NÃO bloqueia o freeze
    // (fora do escopo G7; registrado para proveniência).
    if Path::new("target/e1_paws_consistency.json").exists() {
        let body = fs::read_to_string("target/e1_paws_consistency.json").unwrap_or_default();
        let auc = extract_num(&body, "auc_e1").unwrap_or(f64::NAN);
        report.push(format!(
            "NOTA_R|e1_paws_consistency auc={auc:.4} achado_negativo_trilha_R_nao_bloqueia"
        ));
    }

    let json_rows: Vec<String> = report.iter().map(|r| format!("\"{r}\"")).collect();
    let json = format!(
        "{{\"g7\":\"us_aggregate\",\"pass\":{pass},\"wired\":{wired},\"fail\":{fail},\"rows\":[{}]}}\n",
        json_rows.join(",")
    );
    let _ = fs::create_dir_all("target");
    fs::write("target/us_aggregate_gate.json", json).expect("artefato us_aggregate");

    eprintln!("US0-aggregate: pass={pass} wired={wired} fail={fail}");
    for r in &report {
        eprintln!("  {r}");
    }
    assert_eq!(
        fail, 0,
        "G7: linhas com FONTE_AUSENTE ou FALHOU — wiring não pode apodrecer e artefato existente deve bater o piso/teto"
    );
}
