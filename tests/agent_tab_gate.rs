//! Gate A1 / T-TAB — planilha → `Claim[]` canônico (auth 96).
//!
//! Mede o contrato de `src/agent/tab.rs`:
//! * round-trip de 1000 linhas (aspas, vírgula, CRLF, quebra interna);
//! * OOD de coluna → 100% de abstenção (0 claims inventadas);
//! * 5 arquivos malformados → `Err` explícito (0 panic, 0 linha engolida);
//! * TSV + BOM + CRLF e mapeamento explícito;
//! * auth ≤ 120 e hash de schema determinístico.
//!
//! Artefato: `target/aether_tab_gate.json` (consumido pelo cycle_gate).

#![cfg(feature = "agent")]

use std::path::Path;

use aether_engine::agent::tab::{
    claims_from_tab, facts_jsonl, parse_tabular, parse_tabular_with, TabError, TabMapping, TabRole,
    MAX_LINE_BYTES, TAB_AUTH,
};
use aether_engine::json::Json;

const ROWS: usize = 1000;

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn value_for(i: usize) -> String {
    match i % 5 {
        0 | 4 => format!("v{i}"),
        1 => format!("com, vírgula {i}"),
        2 => format!("com \"aspas\" {i}"),
        _ => format!("linha\nquebrada {i}"),
    }
}

fn build_csv(rows: usize) -> String {
    let mut s = String::from("subject,relation,value\n");
    for i in 0..rows {
        s.push_str(&format!("s{i},tem,{}\r\n", csv_field(&value_for(i))));
    }
    s
}

fn check_roundtrip() -> Result<usize, String> {
    let csv = build_csv(ROWS);
    let doc = parse_tabular(&csv).map_err(|e| format!("parse: {e}"))?;
    if doc.rows.len() != ROWS {
        return Err(format!("linhas: {} != {ROWS}", doc.rows.len()));
    }
    if doc.schema.role_index(TabRole::Subject) != Some(0)
        || doc.schema.role_index(TabRole::Relation) != Some(1)
        || doc.schema.role_index(TabRole::Value) != Some(2)
    {
        return Err("schema não mapeou 0/1/2".into());
    }
    let (claims, abstains) = claims_from_tab(&doc);
    if !abstains.is_empty() {
        return Err(format!("round-trip com {} abstains", abstains.len()));
    }
    if claims.len() != ROWS {
        return Err(format!("claims: {} != {ROWS}", claims.len()));
    }
    for (i, c) in claims.iter().enumerate() {
        if c.subject != format!("s{i}") || c.relation != "tem" || c.value != value_for(i) {
            return Err(format!(
                "linha {i}: ({:?},{:?},{:?})",
                c.subject, c.relation, c.value
            ));
        }
    }
    Ok(claims.len())
}

fn check_facts_jsonl() -> Result<(), String> {
    let csv = build_csv(24);
    let doc = parse_tabular(&csv).map_err(|e| format!("parse: {e}"))?;
    let (claims, _) = claims_from_tab(&doc);
    let text = facts_jsonl(&claims);
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != claims.len() {
        return Err(format!(
            "facts.jsonl: {} linhas != {}",
            lines.len(),
            claims.len()
        ));
    }
    for (i, line) in lines.iter().enumerate() {
        let parsed = Json::parse(line).map_err(|e| format!("json linha {i}: {e}"))?;
        for key in ["subject", "relation", "object"] {
            if parsed.get(key).and_then(|v| v.as_str()).is_none() {
                return Err(format!("facts.jsonl linha {i} sem {key}"));
            }
        }
        let object = parsed.get("object").and_then(|v| v.as_str()).unwrap_or("");
        if object != value_for(i) {
            return Err(format!("facts object linha {i}: {object:?}"));
        }
    }
    Ok(())
}

fn check_ood_abstain() -> Result<usize, String> {
    let csv = "alpha,beta,gamma\n1,2,3\n4,5,6\n";
    let doc = parse_tabular(csv).map_err(|e| format!("parse: {e}"))?;
    if doc.schema.unmapped().len() != 3 {
        return Err("colunas OOD não ficaram Unmapped".into());
    }
    let (claims, abstains) = claims_from_tab(&doc);
    if !claims.is_empty() {
        return Err(format!("OOD gerou {} claims", claims.len()));
    }
    if abstains.len() != doc.rows.len() {
        return Err("OOD não absteve 100% das linhas".into());
    }
    // Meio-schema (só subject) também abstém tudo.
    let partial = "subject,beta,gamma\nA, x, y\n";
    let doc = parse_tabular(partial).map_err(|e| format!("parse partial: {e}"))?;
    let (claims, abstains) = claims_from_tab(&doc);
    if !claims.is_empty() || abstains.len() != 1 {
        return Err("schema parcial não absteve".into());
    }
    Ok(abstains.len())
}

fn check_malformed() -> Result<usize, String> {
    let too_many_cols = format!(
        "{}\n{}\n",
        (0..257)
            .map(|i| format!("c{i}"))
            .collect::<Vec<_>>()
            .join(","),
        (0..257)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let long_line = format!(
        "subject,relation,value\ns,r,{}\n",
        "x".repeat(MAX_LINE_BYTES + 16)
    );
    let cases: [(&str, &str); 5] = [
        ("vazio", ""),
        ("aspa aberta", "subject,relation,value\n\"s,r,v\n"),
        ("linha raggada", "subject,relation,value\ns,r\n"),
        ("colunas demais", too_many_cols.as_str()),
        ("linha longa", long_line.as_str()),
    ];
    let mut errs = 0;
    for (name, csv) in cases {
        match parse_tabular(csv) {
            Err(_) => errs += 1,
            Ok(doc) => {
                return Err(format!(
                    "malformado '{name}' passou (rows={}, claims={})",
                    doc.rows.len(),
                    claims_from_tab(&doc).0.len()
                ))
            }
        }
    }
    // O caso raggado precisa ser precisamente RaggedRow (linha não é engolida).
    if parse_tabular("subject,relation,value\ns,r\n")
        != Err(TabError::RaggedRow {
            row: 2,
            expected: 3,
            found: 2,
        })
    {
        return Err("ragged não retornou RaggedRow(row=2)".into());
    }
    Ok(errs)
}

fn check_tsv_bom_e_mapping() -> Result<(), String> {
    let tsv = "\u{feff}entidade\tpredicado\tobjeto\r\nBrasil\tcapital\tBrasília\r\n";
    let doc = parse_tabular(tsv).map_err(|e| format!("tsv: {e}"))?;
    if doc.dialect.label() != "tsv" {
        return Err(format!("dialeto: {}", doc.dialect.label()));
    }
    let (claims, abstains) = claims_from_tab(&doc);
    if claims.len() != 1 || !abstains.is_empty() {
        return Err("tsv não produziu 1 claim".into());
    }
    if claims[0].subject != "Brasil" || claims[0].value != "Brasília" {
        return Err("tsv: claim errada".into());
    }
    // Header opaco + mapeamento explícito.
    let opaque = "a;b;c\nVec;metodo;push\n";
    let mapping =
        TabMapping::parse("subject=a,relation=b,value=c").map_err(|e| format!("mapping: {e}"))?;
    let doc = parse_tabular_with(opaque, &mapping).map_err(|e| format!("opaque: {e}"))?;
    if doc.dialect.label() != "csv_semicolon" {
        return Err(format!("dialeto ;: {}", doc.dialect.label()));
    }
    let (claims, _) = claims_from_tab(&doc);
    if claims.len() != 1 || claims[0].value != "push" {
        return Err("mapeamento explícito falhou".into());
    }
    // Mapeamento citando coluna inexistente = SchemaBreak (não silêncio).
    let bad = TabMapping::parse("subject=zzz").map_err(|e| format!("mapping: {e}"))?;
    if !matches!(
        parse_tabular_with(opaque, &bad),
        Err(TabError::SchemaBreak { .. })
    ) {
        return Err("mapeamento ausente não quebrou o schema".into());
    }
    Ok(())
}

fn check_auth_e_determinismo() -> Result<(), String> {
    if TAB_AUTH != 96 || TAB_AUTH > 120 {
        return Err(format!("auth fora do teto: {TAB_AUTH}"));
    }
    let csv = build_csv(16);
    let a = parse_tabular(&csv).map_err(|e| format!("parse a: {e}"))?;
    let b = parse_tabular(&csv).map_err(|e| format!("parse b: {e}"))?;
    if a.schema.hash != b.schema.hash || a.schema.hash.len() != 16 {
        return Err(format!(
            "hash instável: {} / {}",
            a.schema.hash, b.schema.hash
        ));
    }
    let (claims, _) = claims_from_tab(&a);
    for c in &claims {
        if c.authority != TAB_AUTH || c.authority > 120 {
            return Err(format!("authority {} > teto", c.authority));
        }
    }
    Ok(())
}

fn check_tool_registry() -> Result<(), String> {
    use aether_engine::agent::tools::default_tools;
    use aether_engine::agent::AgentConfig;
    let tools = default_tools(&AgentConfig::default());
    if !tools.iter().any(|t| t.name() == "tab") {
        return Err("tool `tab` ausente do registry".into());
    }
    Ok(())
}

#[test]
fn a1_t_tab_gate() {
    let mut checks: Vec<(String, bool, String)> = Vec::new();
    let mut extra_rows = 0usize;

    match check_roundtrip() {
        Ok(n) => checks.push(("roundtrip_1000".into(), true, format!("{n} claims"))),
        Err(e) => checks.push(("roundtrip_1000".into(), false, e)),
    }
    match check_facts_jsonl() {
        Ok(()) => checks.push(("facts_jsonl".into(), true, "24 linhas canônicas".into())),
        Err(e) => checks.push(("facts_jsonl".into(), false, e)),
    }
    match check_ood_abstain() {
        Ok(n) => {
            extra_rows += n;
            checks.push(("ood_abstain".into(), true, format!("{n} abstains")));
        }
        Err(e) => checks.push(("ood_abstain".into(), false, e)),
    }
    match check_malformed() {
        Ok(n) => checks.push(("malformados".into(), true, format!("{n}/5 Err"))),
        Err(e) => checks.push(("malformados".into(), false, e)),
    }
    match check_tsv_bom_e_mapping() {
        Ok(()) => checks.push(("tsv_bom_mapping".into(), true, "ok".into())),
        Err(e) => checks.push(("tsv_bom_mapping".into(), false, e)),
    }
    match check_auth_e_determinismo() {
        Ok(()) => checks.push(("auth_determinismo".into(), true, format!("auth={TAB_AUTH}"))),
        Err(e) => checks.push(("auth_determinismo".into(), false, e)),
    }
    match check_tool_registry() {
        Ok(()) => checks.push(("tool_registry".into(), true, "tab registrada".into())),
        Err(e) => checks.push(("tool_registry".into(), false, e)),
    }

    let ok = checks.iter().all(|(_, ok, _)| *ok);
    let artifact = Json::Obj(vec![
        ("gate".to_string(), Json::Str("A1_T_TAB".to_string())),
        ("ok".to_string(), Json::Bool(ok)),
        ("auth".to_string(), Json::Num(f64::from(TAB_AUTH))),
        ("rows_roundtrip".to_string(), Json::Num(ROWS as f64)),
        ("abstain_ood".to_string(), Json::Num(extra_rows as f64)),
        (
            "checks".to_string(),
            Json::Arr(
                checks
                    .iter()
                    .map(|(name, ok, detail)| {
                        Json::Obj(vec![
                            ("name".to_string(), Json::Str(name.clone())),
                            ("ok".to_string(), Json::Bool(*ok)),
                            ("detail".to_string(), Json::Str(detail.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/aether_tab_gate.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, artifact.to_string()).expect("gravar target/aether_tab_gate.json");

    let failures: Vec<String> = checks
        .iter()
        .filter(|(_, ok, _)| !ok)
        .map(|(name, _, detail)| format!("{name}: {detail}"))
        .collect();
    assert!(ok, "gate A1/T-TAB vermelho: {failures:?}");
}
