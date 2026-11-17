//! A rota `/v1/aether/agent`: inferência + coerência pela borda HTTP.
//!
//! Um app de IA não precisa linkar a biblioteca: fala JSON e recebe ação,
//! resposta, certificado e abstenção — as mesmas garantias da CLI.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use aether_engine::agent::SgdbUndoBackend;
use aether_engine::api::json::Json;
use aether_engine::api::{self, ApiConfig};

fn start() -> SocketAddr {
    start_cfg(ApiConfig {
        threads: 1,
        ..ApiConfig::default()
    })
}

fn start_cfg(cfg: ApiConfig) -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, cfg);
    });
    addr
}

/// Arquivo temporário exclusivo do processo/teste.
fn temp_file(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "aether_api_agent_{tag}_{}_{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

/// Define `AETHER_UNDO_JOURNAL` e restaura no `Drop` (o pipeline lê o env).
struct EnvGuard {
    previous: Option<String>,
}

impl EnvGuard {
    fn set_journal(path: &std::path::Path) -> Self {
        let previous = std::env::var("AETHER_UNDO_JOURNAL").ok();
        std::env::set_var("AETHER_UNDO_JOURNAL", path);
        Self { previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("AETHER_UNDO_JOURNAL", value),
            None => std::env::remove_var("AETHER_UNDO_JOURNAL"),
        }
    }
}

fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    request_with_hitl(addr, method, path, body, None)
}

/// Igual a [`request`], com o segredo HITL no cabeçalho `x-aether-hitl`.
fn request_with_hitl(
    addr: SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    hitl: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let hitl = hitl
        .map(|token| format!("x-aether-hitl: {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\n\
         {hitl}content-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    let (head, payload) = response.split_once("\r\n\r\n").expect("separador");
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, payload.to_string())
}

/// Resposta tem de ser JSON válido: o teste olha o **topo** do corpo, não
/// substring (substring passaria mesmo com o campo no lugar errado).
fn json_of(body: &str) -> Json {
    Json::parse(body).unwrap_or_else(|e| panic!("resposta não é JSON ({e}): {body}"))
}

fn bool_field(doc: &Json, key: &str) -> bool {
    doc.get(key)
        .and_then(Json::as_bool)
        .unwrap_or_else(|| panic!("campo `{key}` ausente ou não-booleano: {}", doc.to_text()))
}

/// `false` quando o campo não existe (resposta de tarefa única não tem
/// `pipeline`, e a ausência significa "não é workflow").
fn bool_field_opt(doc: &Json, key: &str) -> bool {
    doc.get(key).and_then(Json::as_bool).unwrap_or(false)
}

fn str_field<'a>(doc: &'a Json, key: &str) -> &'a str {
    doc.get(key)
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("campo `{key}` ausente ou não-string: {}", doc.to_text()))
}

/// Mensagem de erro (`error.message`) da resposta.
fn error_message(doc: &Json) -> String {
    doc.get("error")
        .and_then(|e| e.get("message"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("resposta sem error.message: {}", doc.to_text()))
        .to_string()
}

/// Tipo estável do erro (`error.type`).
fn error_kind(doc: &Json) -> String {
    doc.get("error")
        .and_then(|e| e.get("type"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("resposta sem error.type: {}", doc.to_text()))
        .to_string()
}

/// Fase sintética usada por `remember`/`cue` (par re/im, mínimo aceito).
const PHASES: &str = "[0.1,0.2,0.3,0.4]";

fn remember_body(key: u64, organism: u64) -> String {
    remember_body_with_authority(key, organism, 128)
}

fn remember_body_with_authority(key: u64, organism: u64, authority: u64) -> String {
    format!(
        r#"{{"key":"{key}","phases":{PHASES},
            "scope":{{"organism":{organism},"run":1,"app":1}},"authority":{authority}}}"#
    )
}

fn cue_body(organism: u64) -> String {
    format!(r#"{{"scope":{{"organism":{organism},"run":1,"app":1}},"query":{PHASES}}}"#)
}

fn forget_body(key: u64, organism: u64, authority: u64) -> String {
    format!(
        r#"{{"key":"{key}","scope":{{"organism":{organism},"run":1,"app":1}},
            "authority":{authority},"reason":"teste"}}"#
    )
}

#[test]
fn api_agent_responde_fato_e_abstem_no_desconhecido() {
    let addr = start();
    let (status, health) = request(addr, "GET", "/health", "");
    assert_eq!(status, 200);
    assert!(health.contains("\"agent\":true"), "{health}");

    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"subject":"Brasil","relation":"capital"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"act\""), "{body}");
    assert!(body.contains("\"answer\":\"Brasília\""), "{body}");
    assert!(body.contains("\"experts\":[\"transplant\"]"), "{body}");

    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"subject":"Atlântida","relation":"capital"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"abstain\""), "{body}");
}

#[test]
fn api_agent_resolve_sat_e_qubo_com_certificado() {
    let addr = start();
    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"n":3,"clauses":[[1,2,0],[-1,3,0]]}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"act\""), "{body}");
    assert!(body.contains("\"certificate\""), "{body}");
    assert!(body.contains("\"violations\":0"), "{body}");

    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"n":4,"j":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"h":[1,1,1,1]}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"act\""), "{body}");
    assert!(body.contains("\"answer\":\"++++\""), "{body}");
}

#[test]
fn api_agent_aceita_pipeline_steps() {
    let addr = start();
    let body = r#"{
      "steps":[
        {"kind":"fact","subject":"Brasil","relation":"capital"},
        {"kind":"sat","n":3,"clauses":[[1,2,0],[-1,3,0]]}
      ],
      "budget":{"max_steps":6,"max_ms":5000}
    }"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", body);
    assert_eq!(status, 200, "{resp}");
    let doc = json_of(&resp);
    assert!(bool_field(&doc, "pipeline"), "{resp}");
    assert!(bool_field(&doc, "success"), "{resp}");
    assert_eq!(str_field(&doc, "action"), "act", "{resp}");
    assert!(resp.contains("\"trace\""), "{resp}");
}

#[test]
fn api_agent_pipeline_rejeita_acima_do_cap() {
    let addr = start();
    let too_many = r#"{"steps":[
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"}
    ]}"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", too_many);
    assert_eq!(status, 400, "{resp}");
    assert!(
        resp.contains("8 steps") || resp.contains("pipeline"),
        "{resp}"
    );

    let big_n = r#"{"steps":[{"kind":"sat","n":65,"clauses":[[1,2,0]]}]}"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", big_n);
    assert_eq!(status, 400, "{resp}");
    assert!(resp.contains("n fora") || resp.contains("64"), "{resp}");
}

#[test]
fn api_agent_pipeline_sucesso_em_json_no_topo() {
    let addr = start();
    let body = r#"{
      "pipeline":[
        {"kind":"fact","subject":"Brasil","relation":"capital"},
        {"kind":"sat","n":3,"clauses":[[1,2,0],[-1,3,0]]}
      ],
      "budget":{"max_steps":6,"max_ms":5000}
    }"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", body);
    assert_eq!(status, 200, "{resp}");
    let doc = json_of(&resp);
    assert!(bool_field(&doc, "pipeline"), "{resp}");
    assert!(bool_field(&doc, "success"), "{resp}");
    assert_eq!(str_field(&doc, "action"), "act", "{resp}");
    let steps = doc
        .get("step_count")
        .and_then(Json::as_f64)
        .unwrap_or_default();
    assert!(steps >= 2.0, "step_count inesperado: {resp}");
}

#[test]
fn api_agent_pipeline_abstem_sem_erro_http() {
    let addr = start();
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"steps":[{"kind":"fact","subject":"Atlântida","relation":"capital"}]}"#,
    );
    assert_eq!(status, 200, "abstenção é resposta válida, não erro: {resp}");
    let doc = json_of(&resp);
    assert!(!bool_field(&doc, "success"), "{resp}");
    assert_eq!(str_field(&doc, "action"), "abstain", "{resp}");
    assert!(bool_field(&doc, "pipeline"), "{resp}");
}

#[test]
fn api_agent_pipeline_recusa_payloads_malformados() {
    let addr = start();
    let cases: [(&str, &str); 5] = [
        (r#"{"steps":[]}"#, "pipeline sem steps"),
        (r#"{"steps":[{"kind":"teleport"}]}"#, "kind desconhecido"),
        // Literal 999 com n=3 estourava `Clause3::satisfied` (panic + mutex
        // envenenado): agora é 400 e o servidor segue vivo.
        (
            r#"{"steps":[{"kind":"sat","n":3,"clauses":[[999,1,0]]}]}"#,
            "literal 999",
        ),
        (r#"{"steps":[{"kind":"qubo","n":2,"j":[1,2]}]}"#, "qubo j"),
        (
            r#"{"steps":[{"kind":"sat","n":65,"clauses":[[1,2,0]]}]}"#,
            "sat n fora de 1..=64",
        ),
    ];
    for (body, expected) in cases {
        let (status, resp) = request(addr, "POST", "/v1/aether/agent", body);
        assert_eq!(status, 400, "corpo {body} → {resp}");
        let msg = error_message(&json_of(&resp));
        assert!(msg.contains(expected), "corpo {body} → mensagem {msg:?}");
    }
}

#[test]
fn api_agent_tarefa_unica_recusa_payload_invalido() {
    let addr = start();
    let cases: [(&str, &str); 5] = [
        // QUBO: sem teto, `flat_matrix` reservaria n² floats.
        (r#"{"n":1000000,"j":[]}"#, "qubo n fora de 1..=64"),
        (r#"{"n":65,"j":[],"h":[]}"#, "qubo n fora de 1..=64"),
        // SAT: mesmo teto do pipeline.
        (r#"{"n":65,"clauses":[[1,2,0]]}"#, "sat n fora de 1..=64"),
        // Literal fora do intervalo também na tarefa única.
        (r#"{"n":3,"clauses":[[999,1,0]]}"#, "literal 999"),
        // `h` presente com tamanho errado não vira zeros silenciosos.
        (r#"{"n":2,"j":[0,0,0,0],"h":[1]}"#, "h deve ter 2 elementos"),
    ];
    for (body, expected) in cases {
        let (status, resp) = request(addr, "POST", "/v1/aether/agent", body);
        assert_eq!(status, 400, "corpo {body} → {resp}");
        let msg = error_message(&json_of(&resp));
        assert!(msg.contains(expected), "corpo {body} → mensagem {msg:?}");
    }

    // E o caminho válido continua respondendo.
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"n":3,"clauses":[[1,2,0],[-1,3,0]]}"#,
    );
    assert_eq!(status, 200, "{resp}");
    assert_eq!(str_field(&json_of(&resp), "action"), "act", "{resp}");
}

#[test]
fn api_agent_pipeline_nulo_nao_desvia_tarefa_unica() {
    let addr = start();
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"pipeline":null,"subject":"Brasil","relation":"capital"}"#,
    );
    assert_eq!(status, 200, "`pipeline: null` não é workflow: {resp}");
    let doc = json_of(&resp);
    assert!(!bool_field_opt(&doc, "pipeline"), "{resp}");
    assert_eq!(str_field(&doc, "action"), "act", "{resp}");
    assert_eq!(str_field(&doc, "answer"), "Brasília", "{resp}");
}

#[test]
fn api_agent_continua_vivo_depois_de_corpo_hostil() {
    let addr = start();
    // O corpo que antes panica dentro de `handle` (e envenenava o mutex).
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"steps":[{"kind":"sat","n":3,"clauses":[[999,1,0]]}]}"#,
    );
    assert_eq!(status, 400, "{resp}");

    // Mesma porta, mesmo worker: a borda tem de responder de novo.
    let (status, health) = request(addr, "GET", "/health", "");
    assert_eq!(status, 200, "{health}");

    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"subject":"Brasil","relation":"capital"}"#,
    );
    assert_eq!(status, 200, "servidor morreu após payload hostil: {resp}");
    assert_eq!(str_field(&json_of(&resp), "action"), "act", "{resp}");
}

/// Passo `act` grava via `memory.write`; quando o passo seguinte abstém, o
/// rollback do workflow desfaz a escrita de verdade (journal real, não mock).
#[test]
fn api_agent_pipeline_desfaz_escrita_no_rollback() {
    let journal = temp_file("rollback");
    let _ = std::fs::remove_file(&journal);
    let _env = EnvGuard::set_journal(&journal);
    let addr = start();

    let body = r#"{"steps":[
      {"kind":"act","subject":"k","relation":"rel","query":"v"},
      {"kind":"fact","subject":"Atlântida","relation":"capital"}
    ]}"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", body);
    assert_eq!(status, 200, "{resp}");
    let doc = json_of(&resp);
    assert!(!bool_field(&doc, "success"), "{resp}");
    assert_eq!(str_field(&doc, "action"), "abstain", "{resp}");

    // A escrita aconteceu (linha `write`) e foi compensada (linha `delete`).
    let text = std::fs::read_to_string(&journal).unwrap_or_default();
    assert!(
        text.contains("\"op\":\"write\""),
        "journal sem escrita: {text}"
    );
    assert!(
        text.contains("\"op\":\"delete\""),
        "journal sem undo: {text}"
    );
    assert!(text.contains("k|rel"), "journal não fala da chave: {text}");

    // Reabrir o journal (replay) prova que o valor não sobreviveu ao rollback.
    let backend = SgdbUndoBackend::new(&journal);
    assert!(
        backend.get("k|rel").is_none(),
        "rollback deixou a escrita no journal"
    );

    drop(backend);
    let _ = std::fs::remove_file(&journal);
}

/// Segredo HITL **do servidor** usado nos testes de autoridade.
const HITL_TOKEN: &str = "hitl-secret";

/// Authority 200 = túmulo + HITL: sem o segredo do servidor a rota recusa; com
/// ele, o túmulo vai para o disco **antes** de qualquer remoção e o registro
/// não é purgado no mesmo passo.
#[test]
fn api_agent_forget_200_tombstone_sem_purge() {
    let file = temp_file("forget200");
    let _ = std::fs::remove_file(&file);
    let addr = start_cfg(ApiConfig {
        ledger_path: Some(file.clone()),
        hitl_token: Some(HITL_TOKEN.to_string()),
        threads: 1,
        ..ApiConfig::default()
    });

    let (status, resp) = request(addr, "POST", "/v1/aether/remember", &remember_body(1, 1));
    assert_eq!(status, 200, "{resp}");

    // Sem o segredo HITL, 200 não deleta (o corpo não autoriza nada).
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/memory/forget",
        &forget_body(1, 1, 200),
    );
    assert_eq!(status, 403, "{resp}");
    assert_eq!(error_kind(&json_of(&resp)), "hitl_required", "{resp}");

    let (status, resp) = request_with_hitl(
        addr,
        "POST",
        "/v1/aether/memory/forget",
        &forget_body(1, 1, 200),
        Some(HITL_TOKEN),
    );
    assert_eq!(status, 200, "{resp}");
    let doc = json_of(&resp);
    assert!(!bool_field(&doc, "forgot"), "200 não purga: {resp}");
    assert!(bool_field(&doc, "tombstoned"), "túmulo ausente: {resp}");
    assert!(bool_field(&doc, "hitl"), "200 tem de escalar HITL: {resp}");

    // Túmulo gravado no disco...
    let text = std::fs::read_to_string(&file).expect("ledger");
    assert!(text.contains("\"op\":\"forget\""), "sem túmulo: {text}");
    // ...e o fato continua no ledger (não foi apagado neste passo).
    let (_, health) = request(addr, "GET", "/health", "");
    assert!(health.contains("\"ledger_entries\":1"), "{health}");
    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(resp.contains("\"key\":\"1\""), "fato sumiu no 200: {resp}");

    // Só 255 com o segredo HITL purga (túmulo já está no disco).
    let (status, resp) = request_with_hitl(
        addr,
        "POST",
        "/v1/aether/memory/forget",
        &forget_body(1, 1, 255),
        Some(HITL_TOKEN),
    );
    assert_eq!(status, 200, "{resp}");
    assert!(resp.contains("\"forgot\":true"), "{resp}");
    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(resp.contains("\"abstain\":true"), "ainda cita: {resp}");

    let _ = std::fs::remove_file(&file);
}

/// Rota de memória ignora a autoridade do corpo: fato novo entra em quarentena
/// (96); a rota de ledger confiável preserva a autoridade explícita do host.
#[test]
fn api_agent_remember_ignora_authority_do_cliente() {
    let addr = start_cfg(ApiConfig {
        hitl_token: None,
        threads: 1,
        ..ApiConfig::default()
    });

    // 255 no corpo não vira 255 na memória: é 96, e a resposta sinaliza.
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/memory/remember",
        &remember_body_with_authority(1, 1, 255),
    );
    assert_eq!(status, 200, "{resp}");
    let doc = json_of(&resp);
    let authority = doc.get("authority").and_then(Json::as_u64);
    assert_eq!(authority, Some(96), "autoridade efetiva errada: {resp}");
    let requested = doc.get("authority_requested").and_then(Json::as_u64);
    assert_eq!(requested, Some(255), "pedido não foi registrado: {resp}");

    // A leitura (`cue`) confirma o que foi gravado.
    let (status, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert_eq!(status, 200, "{resp}");
    assert!(resp.contains("\"key\":\"1\""), "{resp}");
    assert!(resp.contains("\"authority\":96"), "{resp}");

    // Mesmo um pedido intermediário (128) não escapa da quarentena.
    let (_, resp) = request(
        addr,
        "POST",
        "/v1/aether/memory/remember",
        &remember_body_with_authority(2, 2, 128),
    );
    assert!(resp.contains("\"authority\":96"), "{resp}");

    // E `forget` < 200 continua recusado como antes.
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/memory/forget",
        &forget_body(1, 1, 128),
    );
    assert_eq!(status, 403, "{resp}");
    assert_eq!(error_kind(&json_of(&resp)), "authority_required", "{resp}");

    // A recusa não apagou o fato.
    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(resp.contains("\"key\":\"1\""), "{resp}");

    // Rota de ledger (organismo/chat): autoridade explícita do host é
    // preservada — é o contrato antigo, documentado em docs/EXTERNAL_API.md.
    let (status, resp) = request(
        addr,
        "POST",
        "/v1/aether/remember",
        &remember_body_with_authority(3, 3, 255),
    );
    assert_eq!(status, 200, "{resp}");
    assert!(resp.contains("\"authority\":255"), "{resp}");
    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(3));
    assert!(resp.contains("\"key\":\"3\""), "{resp}");
    assert!(resp.contains("\"authority\":255"), "{resp}");

    // Na rota de ledger o corpo É o mandato: 255 purga sem header HITL.
    let (status, resp) = request(addr, "POST", "/v1/aether/forget", &forget_body(3, 3, 255));
    assert_eq!(status, 200, "{resp}");
    assert!(resp.contains("\"forgot\":true"), "{resp}");
}

/// 255 purga um fato aprovado por humano, mas **só** com o segredo HITL do
/// servidor: `authority`/`confirm` no corpo não concedem poder.
#[test]
fn api_agent_forget_255_exige_segredo_hitl() {
    let addr = start_cfg(ApiConfig {
        hitl_token: Some(HITL_TOKEN.to_string()),
        threads: 1,
        ..ApiConfig::default()
    });

    // O único caminho para um fato 255 na rota de memória é o HITL do servidor.
    let (status, resp) = request_with_hitl(
        addr,
        "POST",
        "/v1/aether/memory/remember",
        &remember_body_with_authority(9, 1, 255),
        Some(HITL_TOKEN),
    );
    assert_eq!(status, 200, "{resp}");
    assert!(resp.contains("\"authority\":255"), "{resp}");

    // Sem segredo (mesmo pedindo `confirm:true`) a rota recusa.
    let body = r#"{"key":"9","scope":{"organism":1,"run":1,"app":1},
        "authority":255,"confirm":true,"reason":"teste"}"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/memory/forget", body);
    assert_eq!(status, 403, "{resp}");
    assert_eq!(error_kind(&json_of(&resp)), "hitl_required", "{resp}");

    // Segredo errado também não passa.
    let (status, resp) = request_with_hitl(
        addr,
        "POST",
        "/v1/aether/memory/forget",
        &body,
        Some("errado"),
    );
    assert_eq!(status, 403, "{resp}");
    assert_eq!(error_kind(&json_of(&resp)), "hitl_required", "{resp}");

    // Recusa não é purga: o fato continua citável.
    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(
        resp.contains("\"key\":\"9\""),
        "recusa apagou o fato: {resp}"
    );

    // Segredo certo, mas sem store: não há onde gravar o túmulo, então o
    // fato permanece citável (500, não purge silencioso).
    let (status, resp) = request_with_hitl(
        addr,
        "POST",
        "/v1/aether/memory/forget",
        &forget_body(9, 1, 255),
        Some(HITL_TOKEN),
    );
    assert_eq!(status, 500, "{resp}");
    assert_eq!(error_kind(&json_of(&resp)), "ledger_io", "{resp}");
    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(resp.contains("\"key\":\"9\""), "purge sem túmulo: {resp}");
}
