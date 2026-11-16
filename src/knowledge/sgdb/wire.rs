//! Codificação do fato de fase para o dialeto do `neural-sgdb`.
//!
//! O SGDB fala JSON (MCP); a fase é binária (`f32` complexo intercalado). Este
//! arquivo é a **única** fronteira onde bytes viram texto e vice-versa — e o
//! lugar onde o payload ganha o tipo que o banco não tem (`phase`, pedido #1 do
//! Vol. III §8).
//!
//! # Formato do documento (`aether-phase/v1`)
//!
//! ```json
//! {"aether":1,"key":"<32 hex>","layer":4,"cert":{"e":-1.0,"g":0.1,"v":0},
//!  "auth":200,"scope":"org/run/app","tick":42,"wall_ms":1700000000000,
//!  "phase":"<base64 dos bytes f32 re,im>"}
//! ```
//!
//! O `text` enviado ao `remember` é exatamente esta linha. O banco indexa os
//! tokens e as `entities`; o payload binário volta íntegro na `recall` e vira
//! semente de atrator (`cue_into`), como o Vol. III §7 exige.

use crate::core::knowledge_bridge_f1::{Certificate, DraftRecord, Layer, ScopeId};
use crate::json::Json;

/// Identificador do formato no texto e nas entidades.
pub const FORMAT: &str = "aether-phase/v1";

/// Entidade comum a todo fato de fase (o "tipo" que o SGDB não tem).
pub const ENTITY_PHASE: &str = "aether/phase";

/// Teto de memórias por `remember` (limite do próprio SGDB).
pub const MAX_BATCH: usize = 64;

/// Documento de fase — o fato inteiro, sem perda.
#[derive(Clone, Debug, PartialEq)]
pub struct PhaseDoc {
    /// Endereço por conteúdo (`u128`, sem string no motor).
    pub key: u128,
    /// Camada lógica (`L2`, `L4`, `L6`).
    pub layer: Layer,
    /// Certificado de factibilidade.
    pub certificate: Certificate,
    /// Autoridade social (`0..=255`).
    pub authority: u8,
    /// Escopo de origem.
    pub scope: ScopeId,
    /// Relógio interno (dinâmica).
    pub tick: u64,
    /// Relógio externo (TTL/decay).
    pub wall_ms: u64,
    /// Bytes crus de fase (`f32` complexo intercalado).
    pub payload: Vec<u8>,
    /// Chave de armazenamento devolvida pelo banco (só depois do `remember`).
    pub storage_key: Option<String>,
}

impl PhaseDoc {
    /// Converte um registro drenado do `T1` em documento de fase.
    pub fn from_record(record: &DraftRecord) -> Self {
        Self {
            key: record.key,
            layer: record.layer,
            certificate: record.certificate,
            authority: record.authority,
            scope: record.scope,
            tick: record.tick,
            wall_ms: record.wall_ms,
            payload: record.payload.clone(),
            storage_key: None,
        }
    }

    /// Texto que vai no campo `text` do `remember` (uma linha JSON).
    pub fn to_text(&self) -> String {
        self.to_json().to_text()
    }

    /// Objeto JSON do documento.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("aether", Json::num(1.0)),
            ("format", Json::str(FORMAT)),
            ("key", Json::str(key_hex(self.key))),
            ("layer", Json::num(f64::from(self.layer.code()))),
            (
                "cert",
                Json::obj([
                    ("e", Json::num(f64::from(self.certificate.energy))),
                    ("g", Json::num(f64::from(self.certificate.gap))),
                    ("v", Json::num(f64::from(self.certificate.violations))),
                ]),
            ),
            ("auth", Json::num(f64::from(self.authority))),
            ("scope", Json::str(scope_text(self.scope))),
            ("tick", Json::num(self.tick as f64)),
            ("wall_ms", Json::num(self.wall_ms as f64)),
            ("phase", Json::str(b64_encode(&self.payload))),
        ])
    }

    /// Reconstruí o documento a partir do texto devolvido na `recall`.
    ///
    /// Devolve `None` para texto que não é nosso (o banco pode conter memórias
    /// de outros autores no mesmo escopo).
    pub fn parse(text: &str) -> Option<Self> {
        let json = Json::parse(text.trim()).ok()?;
        Self::from_json(&json)
    }

    /// Reconstruí o documento a partir do objeto JSON.
    pub fn from_json(json: &Json) -> Option<Self> {
        if json
            .get("format")
            .and_then(|v| v.as_str())
            .is_some_and(|f| f != FORMAT)
        {
            return None;
        }
        let key = json
            .get("key")
            .and_then(|v| v.as_str())
            .and_then(parse_key_hex)?;
        let layer = json
            .get("layer")
            .and_then(|v| v.as_u64())
            .and_then(|v| Layer::from_code(v as u8))?;
        let cert = json.get("cert")?;
        let certificate = Certificate {
            energy: cert.get("e").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            gap: cert.get("g").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            violations: cert.get("v").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        };
        let authority = json
            .get("auth")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .min(255) as u8;
        let scope = json
            .get("scope")
            .and_then(|v| v.as_str())
            .and_then(parse_scope_text)?;
        let payload = json
            .get("phase")
            .and_then(|v| v.as_str())
            .and_then(b64_decode)?;
        Some(Self {
            key,
            layer,
            certificate,
            authority,
            scope,
            tick: json.get("tick").and_then(|v| v.as_u64()).unwrap_or(0),
            wall_ms: json.get("wall_ms").and_then(|v| v.as_u64()).unwrap_or(0),
            payload,
            storage_key: None,
        })
    }

    /// Entidades do fato: escopo, tipo, camada e chave de conteúdo.
    ///
    /// O SGDB indexa `entities` em 1 hop; é por elas que a `recall` de um
    /// organismo não vê o outro (R8) — a chave textual é só um alias.
    pub fn entities(&self) -> Vec<String> {
        vec![
            scope_entity(self.scope),
            ENTITY_PHASE.to_string(),
            format!("aether/layer/L{}", self.layer.code()),
            format!("aether/key/{}", key_hex(self.key)),
        ]
    }

    /// Argumentos de **uma** memória para o `remember`.
    pub fn memory_args(&self, cfg: &RememberConfig<'_>) -> Json {
        let mut pairs = vec![
            ("text".to_string(), Json::str(self.to_text())),
            ("type".to_string(), Json::str("json")),
            (
                "entities".to_string(),
                Json::Arr(self.entities().into_iter().map(Json::Str).collect()),
            ),
            ("if_exists".to_string(), Json::str(cfg.if_exists)),
            ("scope_app".to_string(), Json::str(cfg.scope_app)),
            ("scope_run".to_string(), Json::str(scope_run(self.scope))),
            ("scope_agent".to_string(), Json::str(cfg.scope_agent)),
            // `index_key` está fora do escopo do payload: o nome da chave é
            // derivado do conteúdo e não ajuda a busca semântica.
        ];
        if let Some(prefix) = cfg.key_prefix {
            pairs.push((
                "key".to_string(),
                Json::str(format!("{prefix}{}", key_hex(self.key))),
            ));
        }
        Json::Obj(pairs)
    }

    /// Dims complexas do payload (`f32` re + `f32` im = 8 bytes).
    pub fn dims(&self) -> usize {
        self.payload.len() / 8
    }
}

/// Configuração aplicada a cada memória no `remember`.
#[derive(Clone, Copy, Debug)]
pub struct RememberConfig<'a> {
    /// `scope_app` do banco (default `AETHER`).
    pub scope_app: &'a str,
    /// `scope_agent` do banco.
    pub scope_agent: &'a str,
    /// `add` (doutrina ADD-only) ou `reinforce` (evita duplicata idêntica).
    pub if_exists: &'a str,
    /// Prefixo de chave crua (`None` = o banco gera `md/L3/<key>`).
    pub key_prefix: Option<&'a str>,
}

/// Escopo textual do evento (`org/run/app`), estável entre `T0` e `T2`.
pub fn scope_text(scope: ScopeId) -> String {
    format!("{}/{}/{}", scope.organism_id, scope.run_id, scope.app_tag)
}

/// Inverso de [`scope_text`] (`None` se malformado).
pub fn parse_scope_text(text: &str) -> Option<ScopeId> {
    let mut it = text.split('/');
    let organism_id = it.next()?.parse().ok()?;
    let run_id = it.next()?.parse().ok()?;
    let app_tag = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(ScopeId::new(organism_id, run_id, app_tag))
}

/// Entidade de escopo (`aether/scope/<org>/<run>/<app>`).
pub fn scope_entity(scope: ScopeId) -> String {
    format!("aether/scope/{}", scope_text(scope))
}

/// `scope_run` do banco: organismo **e** run (runs de organismos distintos
/// podem coincidir; o escopo nunca é comparado só por `run_id`).
pub fn scope_run(scope: ScopeId) -> String {
    format!("org:{}/run:{}", scope.organism_id, scope.run_id)
}

/// Chave de conteúdo de um payload (FNV-1a 128 — determinística, sem deps).
pub fn content_key(payload: &[u8]) -> u128 {
    let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for &b in payload {
        h ^= u128::from(b);
        h = h.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    h
}

/// Hex minúsculo de 32 dígitos.
pub fn key_hex(key: u128) -> String {
    format!("{key:032x}")
}

/// Inverso de [`key_hex`].
pub fn parse_key_hex(text: &str) -> Option<u128> {
    if text.len() != 32 {
        return None;
    }
    u128::from_str_radix(text, 16).ok()
}

/// Chave `u64` para [`crate::core::certificate::Evidence`] (dobra XOR).
pub fn evidence_id(key: u128) -> u64 {
    (key as u64) ^ ((key >> 64) as u64)
}

// ---------------------------------------------------------------------------
// Base64 (RFC 4648, com padding). Sem dependência, sem tabela alocada.
// ---------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Codifica bytes em base64 padrão.
pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(B64[(b0 >> 2) as usize] as char);
        out.push(B64[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[(b2 & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Decodifica base64 padrão; `None` para texto inválido.
pub fn b64_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        let mut pad = 0usize;
        for (i, &c) in chunk.iter().enumerate() {
            if c == b'=' {
                pad += 1;
                vals[i] = 0;
            } else {
                vals[i] = b64_value(c)?;
            }
        }
        if pad > 2 {
            return None;
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if pad < 2 {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if pad < 1 {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Some(out)
}

fn b64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Respostas do MCP
// ---------------------------------------------------------------------------

/// Extrai o JSON útil de um `result` de `tools/call`.
///
/// O MCP embrulha o retorno em `content: [{type:"text", text:"..."}]`; algumas
/// bordas devolvem o objeto direto. As duas formas são aceitas.
pub fn payload_json(result: &Json) -> Json {
    if let Some(items) = result.get("content").and_then(|c| c.as_arr()) {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    if let Ok(parsed) = Json::parse(text) {
                        return parsed;
                    }
                }
            }
        }
    }
    result.clone()
}

/// Lista de hits de uma resposta de `recall`.
pub fn hits_json(result: &Json) -> Vec<Json> {
    let json = payload_json(result);
    if let Some(arr) = json.as_arr() {
        return arr.to_vec();
    }
    for name in ["hits", "results", "memories", "items"] {
        if let Some(arr) = json.get(name).and_then(|v| v.as_arr()) {
            return arr.to_vec();
        }
    }
    Vec::new()
}

/// Um hit decodificado, pronto para o motor.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryHit {
    /// Documento de fase (payload íntegro).
    pub doc: PhaseDoc,
    /// Chave de armazenamento do banco (para `set_ttl`/`forget`).
    pub storage_key: Option<String>,
    /// Score de recuperação (`0..=1`, quando disponível).
    pub score: f32,
    /// Autoridade efetiva (do hit, com fallback para o documento).
    pub authority: u8,
}

/// Decodifica um hit cru do banco.
pub fn parse_hit(json: &Json) -> Option<MemoryHit> {
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .or_else(|| {
            json.get("payload")
                .and_then(|p| p.get("text"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| json.as_str())?;
    let mut doc = PhaseDoc::parse(text)?;
    let storage_key = json
        .get("key")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("storage_key").and_then(|v| v.as_str()))
        .map(str::to_string);
    doc.storage_key = storage_key.clone();
    let score = json
        .get("score")
        .and_then(|v| v.as_f64())
        .or_else(|| json.get("relevance").and_then(|v| v.as_f64()))
        .or_else(|| json.get("similarity").and_then(|v| v.as_f64()))
        .unwrap_or(1.0) as f32;
    let authority = json
        .get("authority")
        .and_then(|v| v.as_u64())
        .map(|v| v.min(255) as u8)
        .unwrap_or(doc.authority);
    Some(MemoryHit {
        doc,
        storage_key,
        score,
        authority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> PhaseDoc {
        let mut payload = Vec::new();
        for i in 0..4 {
            payload.extend_from_slice(&(i as f32).to_le_bytes());
            payload.extend_from_slice(&(-(i as f32)).to_le_bytes());
        }
        PhaseDoc {
            key: content_key(&payload),
            layer: Layer::L4Semantic,
            certificate: Certificate {
                energy: -1.5,
                gap: 0.125,
                violations: 0,
            },
            authority: 200,
            scope: ScopeId::new(7, 3, 11),
            tick: 42,
            wall_ms: 1_700_000_000_000,
            payload,
            storage_key: None,
        }
    }

    #[test]
    fn base64_round_trip_todos_os_restos() {
        for len in 0..32usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
            let enc = b64_encode(&data);
            assert_eq!(b64_decode(&enc).as_deref(), Some(data.as_slice()));
        }
        assert_eq!(b64_decode("A"), None);
        assert_eq!(b64_decode("!!!!"), None);
    }

    #[test]
    fn documento_faz_round_trip_sem_perda() {
        let original = doc();
        let text = original.to_text();
        let back = PhaseDoc::parse(&text).expect("parse");
        assert_eq!(back, original);
        assert_eq!(back.dims(), 4);
    }

    #[test]
    fn texto_estranho_nao_vira_documento() {
        assert!(PhaseDoc::parse("Brasil, capital, Brasília").is_none());
        assert!(PhaseDoc::parse("{\"format\":\"outro\"}").is_none());
        assert!(PhaseDoc::parse("").is_none());
    }

    #[test]
    fn escopo_textual_faz_round_trip() {
        let scope = ScopeId::new(9, 1, 2);
        let text = scope_text(scope);
        assert_eq!(parse_scope_text(&text), Some(scope));
        assert_eq!(parse_scope_text("9/1"), None);
        assert_eq!(parse_scope_text("9/1/2/3"), None);
        assert!(scope_entity(scope).starts_with("aether/scope/"));
    }

    #[test]
    fn chave_de_conteudo_e_estavel_e_sensivel() {
        let a = content_key(b"fase");
        assert_eq!(a, content_key(b"fase"));
        assert_ne!(a, content_key(b"fase "));
        assert_eq!(parse_key_hex(&key_hex(a)), Some(a));
        assert_eq!(parse_key_hex("xyz"), None);
    }

    #[test]
    fn hit_aceita_as_duas_formas_do_mcp() {
        let d = doc();
        let hit = Json::obj([("text", Json::str(d.to_text())), ("score", Json::num(0.75))]);
        let wrapped = Json::obj([(
            "content",
            Json::Arr(vec![Json::obj([
                ("type", Json::str("text")),
                (
                    "text",
                    Json::str(Json::obj([("hits", Json::Arr(vec![hit.clone()]))]).to_text()),
                ),
            ])]),
        )]);
        let hits = hits_json(&wrapped);
        assert_eq!(hits.len(), 1);
        let parsed = parse_hit(&hits[0]).expect("hit");
        assert_eq!(parsed.doc, d);
        assert!((parsed.score - 0.75).abs() < 1e-6);
    }
}
