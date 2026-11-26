//! A5 / T-HIER — roteamento LSH sobre o bank HV + re-rank f32 top-32.
//!
//! O MoE plano varre todos os átomos em Hamming. O caminho hierárquico coleta
//! candidatos pelo multi-probe LSH do banco e só então re-rankeia em float —
//! mesmo `top_k` do B2 e mesma verdade (a query `i` tem o fato `i`).
//!
//! GO (não negociável, medido e não estimado): `recall1` do caminho LSH **não
//! pode ficar abaixo** do caminho plano, com `p99` menor. Enquanto o gate não
//! medir isso, [`HierMoeStatus::may_enable`] é `false` e nada entra no caminho
//! default. As **duas** direções do fato são medidas: fwd `s1|rel|s2` (o bank do
//! round) e rev `s2|rel|s1` (o lado que fecha o B2, `data/b2_two_directions.json`).
//! [`HierMoeStatus::both_directions_ok`] é a evidência; não é a autorização.

use std::path::Path;
use std::time::Instant;

use crate::core::hv::HvBank;
use crate::json::Json;

/// Path do artefato de status/medida.
pub const HIER_MOE_STATUS_PATH: &str = "target/aether_hier_moe_status.json";

/// Round PAWS com bank HV + `emb/bank.f32` + queries `s2`.
pub const DEFAULT_ROUND: &str = "data/rounds/20261117_b0_paws";

/// Round reverso (`s2|rel|s1`) gerado por `tools/a5_rev_round.py` em `target/`.
pub const REV_ROUND: &str = "target/a5_rev_round";

/// Candidatos que descem para o re-rank float (mesmo top-32 do B2).
pub const RERANK_TOP: usize = 32;

/// Uma configuração LSH a medir: `tables` tabelas × `bits` bits por tabela,
/// `probes` vizinhos Hamming-1 por tabela.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LshConfig {
    /// Número de tabelas.
    pub tables: usize,
    /// Bits por tabela (1..=16).
    pub bits: usize,
    /// Buckets vizinhos por tabela (0 = só o bucket exato).
    pub probes: usize,
}

impl LshConfig {
    /// Constrói uma configuração.
    pub const fn new(tables: usize, bits: usize, probes: usize) -> Self {
        Self {
            tables,
            bits,
            probes,
        }
    }

    /// Rótulo estável para o artefato (`tabelas x bits x probes`).
    pub fn label(&self) -> String {
        format!("{}x{}x{}", self.tables, self.bits, self.probes)
    }
}

/// Banco + queries do round (HV para rotear, f32 normalizado para re-rankear).
#[derive(Debug, Clone)]
pub struct Round {
    /// Bank HV (átomos de 10.240 bits).
    pub bank: HvBank,
    /// Docs f32 L2-normalizados, alinhados 1:1 com os átomos.
    pub docs: Vec<Vec<f32>>,
    /// Queries HV achatadas (`n × words`).
    pub q_hv: Vec<u64>,
    /// Queries f32 achatadas e normalizadas (`n × dim`).
    pub q_f32: Vec<f32>,
    /// Palavras por átomo/query HV.
    pub words: usize,
    /// Dimensão do embedding.
    pub dim: usize,
}

impl Round {
    /// Carrega `hv/bank.bin` + `.idx`, `emb/bank.f32`, `s2_query_hv.bin` e
    /// `s2_query_f32.bin` de um round. Ausência de qualquer um é erro (o gate
    /// chama isto só quando os arquivos existem).
    pub fn load(dir: &Path) -> Result<Self, String> {
        let bank = HvBank::load(
            &dir.join("hv").join("bank.bin"),
            &dir.join("hv").join("bank.idx"),
        )?;
        let (n, dim, mut docs) = read_f32(&dir.join("emb").join("bank.f32"))?;
        if n != bank.count() {
            return Err(format!("bank.f32 n={n} != bank.count={}", bank.count()));
        }
        for d in &mut docs {
            normalize(d);
        }
        let words = bank.words;
        let hv_raw = std::fs::read(dir.join("s2_query_hv.bin")).map_err(|e| e.to_string())?;
        let q_hv: Vec<u64> = hv_raw
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
            .collect();
        if q_hv.len() % words != 0 {
            return Err(format!(
                "s2_query_hv.bin {} u64 não múltiplo de {words}",
                q_hv.len()
            ));
        }
        let (nq, dimq, mut qs) = read_f32_loose(&dir.join("s2_query_f32.bin"), n, dim)?;
        if nq != q_hv.len() / words || dimq != dim {
            return Err(format!(
                "queries desalinhadas nq={nq} hv={} dimq={dimq}",
                q_hv.len() / words
            ));
        }
        for q in &mut qs {
            normalize(q);
        }
        Ok(Self {
            bank,
            docs,
            q_hv,
            q_f32: qs.concat(),
            words,
            dim,
        })
    }

    /// Queries disponíveis.
    pub fn len(&self) -> usize {
        self.q_hv.len() / self.words
    }

    /// Sem queries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Query HV `i`.
    pub fn query_hv(&self, i: usize) -> &[u64] {
        &self.q_hv[i * self.words..(i + 1) * self.words]
    }

    /// Query f32 `i`.
    pub fn query_f32(&self, i: usize) -> &[f32] {
        &self.q_f32[i * self.dim..(i + 1) * self.dim]
    }
}

/// Medição de um caminho: `recall@1` (o doc certo vence o re-rank) e p99 do
/// roteamento+re-rank por query.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PathMeasure {
    /// Queries medidas.
    pub n: usize,
    /// Fração em que o doc certo venceu.
    pub recall1: f64,
    /// p99 por query, em microssegundos (build de teste, não release).
    pub p99_us: u128,
    /// Átomos considerados por query antes do corte em `k` (média).
    pub candidates_avg: f64,
}

/// Varredura exata (baseline "MoE plano"): todos os átomos em Hamming.
pub fn top_k_exact(bank: &HvBank, q: &[u64], k: usize) -> Vec<usize> {
    let mut scored: Vec<(u32, usize)> = (0..bank.count())
        .map(|i| (HvBank::hamming(q, bank.atom_words(i)), i))
        .collect();
    scored.sort_unstable_by_key(|t| t.0);
    scored.truncate(k.min(scored.len()));
    scored.into_iter().map(|t| t.1).collect()
}

/// Reordena um pool de índices por Hamming e devolve os `k` melhores.
pub fn top_k_from_pool(bank: &HvBank, q: &[u64], pool: &[usize], k: usize) -> Vec<usize> {
    let mut scored: Vec<(u32, usize)> = pool
        .iter()
        .map(|&i| (HvBank::hamming(q, bank.atom_words(i)), i))
        .collect();
    scored.sort_unstable_by_key(|t| t.0);
    scored.truncate(k.min(scored.len()));
    scored.into_iter().map(|t| t.1).collect()
}

/// Candidatos LSH (multi-probe) reordenados por Hamming, limitados a `k`.
pub fn top_k_lsh(bank: &HvBank, q: &[u64], k: usize) -> Vec<usize> {
    top_k_from_pool(bank, q, &bank.lsh_candidate_indices(q), k)
}

/// Re-rank f32 (cosseno) sobre os candidatos; devolve o vencedor.
pub fn rerank(round: &Round, q: &[f32], cand: &[usize], top: usize) -> Option<usize> {
    cand.iter()
        .take(top)
        .map(|&j| (dot(q, &round.docs[j]), j))
        .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|t| t.1)
}

/// Mede um caminho (`use_lsh = false` → MoE plano) com `k` candidatos.
pub fn measure(round: &Round, n: usize, k: usize, use_lsh: bool) -> PathMeasure {
    let n = n.min(round.len());
    if n == 0 {
        return PathMeasure::default();
    }
    let mut hits = 0usize;
    let mut cand_total = 0usize;
    let mut us = Vec::with_capacity(n);
    for i in 0..n {
        let q_hv = round.query_hv(i);
        let t0 = Instant::now();
        // `pool` = o que o roteador realmente toca antes de cortar em `k`.
        let (cand, pool) = if use_lsh {
            let pool = round.bank.lsh_candidate_indices(q_hv);
            (top_k_from_pool(&round.bank, q_hv, &pool, k), pool.len())
        } else {
            (top_k_exact(&round.bank, q_hv, k), round.bank.count())
        };
        cand_total += pool;
        let winner = rerank(round, round.query_f32(i), &cand, RERANK_TOP);
        us.push(t0.elapsed().as_micros());
        if winner == Some(i) {
            hits += 1;
        }
    }
    us.sort_unstable();
    PathMeasure {
        n,
        recall1: hits as f64 / n as f64,
        p99_us: us[(us.len() * 99 / 100).min(us.len() - 1)],
        candidates_avg: cand_total as f64 / n as f64,
    }
}

/// Mede o MoE plano e cada configuração LSH sobre o mesmo banco.
pub fn sweep(
    round: &mut Round,
    n: usize,
    k: usize,
    configs: &[LshConfig],
) -> (PathMeasure, Vec<(LshConfig, PathMeasure)>) {
    let plain = measure(round, n, k, false);
    let mut rows = Vec::with_capacity(configs.len());
    for cfg in configs {
        round
            .bank
            .build_lsh_probes(cfg.tables, cfg.bits, cfg.probes);
        rows.push((*cfg, measure(round, n, k, true)));
    }
    (plain, rows)
}

/// A melhor config que não perde recall e ganha p99? `None` se nenhuma passa.
pub fn best_passing(
    plain: PathMeasure,
    rows: &[(LshConfig, PathMeasure)],
) -> Option<(LshConfig, PathMeasure)> {
    rows.iter()
        .filter(|(_, m)| m.n > 0 && m.recall1 >= plain.recall1 && m.p99_us < plain.p99_us)
        .min_by_key(|(_, m)| m.p99_us)
        .copied()
}

/// Arredonda para 4 casas, como o resto do artefato.
fn num4(v: f64) -> Json {
    Json::Num((v * 10_000.0).round() / 10_000.0)
}

/// JSON de uma medida (`Null` quando ausente).
fn measure_json(m: Option<PathMeasure>) -> Json {
    match m {
        Some(m) => Json::obj([
            ("n", Json::Num(m.n as f64)),
            ("recall1", num4(m.recall1)),
            ("p99_us", Json::Num(m.p99_us as f64)),
            ("candidates_avg", num4(m.candidates_avg)),
        ]),
        None => Json::Null,
    }
}

/// Medida de uma direção do A5: fwd `s1|rel|s2` ou rev `s2|rel|s1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionMeasure {
    /// Configuração LSH medida.
    pub config: LshConfig,
    /// MoE plano (varredura exata em Hamming).
    pub plain: PathMeasure,
    /// LSH multi-probe + re-rank f32.
    pub lsh: PathMeasure,
}

impl DirectionMeasure {
    /// GO da direção: recall não menor **e** p99 menor, com medida real.
    pub fn lsh_ok(&self) -> bool {
        let (p, l) = (self.plain, self.lsh);
        p.n > 0 && l.n > 0 && l.recall1 >= p.recall1 && l.p99_us < p.p99_us
    }

    /// JSON da direção no artefato.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("lsh_config", Json::str(self.config.label())),
            ("plain", measure_json(Some(self.plain))),
            ("lsh_rerank_f32", measure_json(Some(self.lsh))),
            ("lsh_ok", Json::Bool(self.lsh_ok())),
        ])
    }
}

/// Status/medida do A5 — é o que o artefato e o `may_enable` leem.
#[derive(Clone, Copy, Debug)]
pub struct HierMoeStatus {
    /// Feature `hier_moe` compilada.
    pub feature_on: bool,
    /// Env `AETHER_HIER_MOE` ligada.
    pub env_on: bool,
    /// Bank acima da capacidade E1B (`0,4 × D`).
    pub over_capacity: bool,
    /// Candidatos por query antes do re-rank.
    pub k: usize,
    /// Direção fwd (`s1|rel|s2`) medida pelo gate.
    pub fwd: Option<DirectionMeasure>,
    /// Direção reversa (`s2|rel|s1`), quando o round rev é medido.
    pub rev: Option<DirectionMeasure>,
}

impl HierMoeStatus {
    /// Flags de runtime, sem medida (default: off).
    pub fn from_env() -> Self {
        let env_on = matches!(
            std::env::var("AETHER_HIER_MOE")
                .ok()
                .as_deref()
                .map(|s| s.trim().to_ascii_lowercase())
                .as_deref(),
            Some("1" | "true" | "yes" | "on")
        );
        Self {
            feature_on: cfg!(feature = "hier_moe"),
            env_on,
            over_capacity: false,
            k: 0,
            fwd: None,
            rev: None,
        }
    }

    /// Anexa a medida fwd do gate.
    pub fn measured(&mut self, over_capacity: bool, k: usize, fwd: DirectionMeasure) -> Self {
        self.over_capacity = over_capacity;
        self.k = k;
        self.fwd = Some(fwd);
        *self
    }

    /// GO do A5 na direção fwd: recall não menor **e** p99 menor, com n > 0.
    pub fn lsh_ok(&self) -> bool {
        self.fwd.is_some_and(|f| f.lsh_ok())
    }

    /// GO nas duas direções: fwd verde **e** rev medida e verde. É a evidência
    /// que autoriza o A5 no default; sem a medida rev, não autoriza.
    pub fn both_directions_ok(&self) -> bool {
        self.lsh_ok() && self.rev.is_some_and(|r| r.lsh_ok())
    }

    /// Só liga com feature + env + GO medido. Nunca no caminho default.
    pub fn may_enable(&self) -> bool {
        self.feature_on && self.env_on && self.lsh_ok()
    }

    /// JSON do artefato.
    pub fn to_json(&self) -> Json {
        let fwd = self.fwd;
        Json::obj([
            ("tomada", Json::str("T-MOE-HIER")),
            ("feature", Json::Bool(self.feature_on)),
            ("env_AETHER_HIER_MOE", Json::Bool(self.env_on)),
            ("over_capacity", Json::Bool(self.over_capacity)),
            ("k", Json::Num(self.k as f64)),
            (
                "lsh_config",
                fwd.map(|d| Json::str(d.config.label()))
                    .unwrap_or(Json::Null),
            ),
            ("plain", measure_json(fwd.map(|d| d.plain))),
            ("lsh_rerank_f32", measure_json(fwd.map(|d| d.lsh))),
            ("lsh_ok", Json::Bool(self.lsh_ok())),
            ("rev", self.rev.map(|r| r.to_json()).unwrap_or(Json::Null)),
            ("both_directions_ok", Json::Bool(self.both_directions_ok())),
            ("may_enable", Json::Bool(self.may_enable())),
            ("default_path", Json::Bool(false)),
            (
                "b2_precondition",
                Json::str("rev 0,9170 em data/b2_two_directions.json, medido pelo A5 em `rev`"),
            ),
            ("blocks_tnl_in", Json::Bool(false)),
        ])
    }
}

/// Lê um `.f32` com header `(n, dim)`.
fn read_f32(path: &Path) -> Result<(usize, usize, Vec<Vec<f32>>), String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if raw.len() < 8 {
        return Err(format!("{}: curto", path.display()));
    }
    let n = u32::from_le_bytes(raw[0..4].try_into().unwrap()) as usize;
    let d = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
    if raw.len() != 8 + n * d * 4 {
        return Err(format!("{}: payload != {n}x{d}", path.display()));
    }
    let rows = raw[8..]
        .chunks_exact(d * 4)
        .map(|r| {
            r.chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect()
        })
        .collect();
    Ok((n, d, rows))
}

/// Lê `.f32` cru (sem header) ou com header, exigindo `n × dim`.
fn read_f32_loose(
    path: &Path,
    n: usize,
    dim: usize,
) -> Result<(usize, usize, Vec<Vec<f32>>), String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let body = if raw.len() == n * dim * 4 {
        &raw[..]
    } else if raw.len() == 8 + n * dim * 4 {
        &raw[8..]
    } else {
        return Err(format!(
            "{}: {} bytes != {n}x{dim}",
            path.display(),
            raw.len()
        ));
    };
    let rows = body
        .chunks_exact(dim * 4)
        .map(|r| {
            r.chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect()
        })
        .collect();
    Ok((n, dim, rows))
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn normalize(v: &mut [f32]) {
    let n = dot(v, v).sqrt();
    if n > 1e-12 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dm(plain: PathMeasure, lsh: PathMeasure) -> DirectionMeasure {
        DirectionMeasure {
            config: LshConfig::new(8, 8, 8),
            plain,
            lsh,
        }
    }

    #[test]
    fn status_sem_medida_nao_liga() {
        let s = HierMoeStatus::from_env();
        assert!(!s.lsh_ok());
        assert!(!s.both_directions_ok());
        assert!(!s.may_enable());
        let j = s.to_json().to_string();
        assert!(j.contains("\"may_enable\":false"));
        assert!(j.contains("\"default_path\":false"));
        assert!(j.contains("\"blocks_tnl_in\":false"));
    }

    #[test]
    fn go_exige_recall_e_p99() {
        let mut s = HierMoeStatus::from_env();
        let plain = PathMeasure {
            n: 100,
            recall1: 0.80,
            p99_us: 900,
            candidates_avg: 32.0,
        };
        // recall menor: reprova mesmo com p99 melhor.
        let lower = PathMeasure {
            recall1: 0.79,
            ..plain
        };
        s.measured(false, 32, dm(plain, lower));
        assert!(!s.lsh_ok());
        // p99 maior: reprova mesmo com recall igual.
        let slower = PathMeasure {
            p99_us: 950,
            ..plain
        };
        s.measured(false, 32, dm(plain, slower));
        assert!(!s.lsh_ok());
        // recall igual e p99 menor: passa o GO.
        let faster = PathMeasure {
            p99_us: 120,
            ..plain
        };
        s.measured(false, 32, dm(plain, faster));
        assert!(s.lsh_ok());
    }

    #[test]
    fn rev_ausente_ou_vermelha_nao_autoriza() {
        let mut s = HierMoeStatus::from_env();
        let plain = PathMeasure {
            n: 100,
            recall1: 0.77,
            p99_us: 900,
            candidates_avg: 32.0,
        };
        let faster = PathMeasure {
            p99_us: 400,
            ..plain
        };
        s.measured(false, 32, dm(plain, faster));
        assert!(s.lsh_ok());
        assert!(
            !s.both_directions_ok(),
            "sem medida rev não autoriza as duas"
        );
        // rev verde: autoriza.
        let rev_ok = PathMeasure {
            recall1: 0.92,
            p99_us: 420,
            ..plain
        };
        s.rev = Some(dm(plain, rev_ok));
        assert!(s.both_directions_ok());
        // rev com p99 maior que o plano: reprova, mesmo com recall alto.
        let rev_slow = PathMeasure {
            recall1: 0.92,
            p99_us: 900,
            ..plain
        };
        s.rev = Some(dm(plain, rev_slow));
        assert!(!s.both_directions_ok());
        let j = s.to_json().to_string();
        assert!(j.contains("\"both_directions_ok\":false"));
    }
}
