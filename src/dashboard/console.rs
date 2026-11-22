//! Aba Console IA — acionamento manual estilo chat (cite / act / abstain).
//!
//! Não é chatbot LLM: fala com `POST /v1/chat/completions` da API local e
//! mostra evidência tipada. Sem API no ar → erro acionável (como subir).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use egui::{self, Color32, RichText, ScrollArea, TextEdit};
use serde_json::{json, Value};

/// Papel de uma bolha no histórico.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Aether,
    System,
}

/// Uma mensagem do histórico.
#[derive(Clone, Debug)]
pub struct ChatMsg {
    pub role: ChatRole,
    pub text: String,
    /// `cite` | `act` | `abstain` | `error` | vazio (user/system).
    pub verdict: String,
}

/// Estado da aba Console (mutável pela UI e pelo worker HTTP).
#[derive(Debug)]
pub struct ConsolePane {
    pub base_url: String,
    pub organism: u64,
    pub run: u64,
    pub app: u64,
    pub input: String,
    pub messages: Vec<ChatMsg>,
    pub busy: bool,
    /// Resposta pendente do worker (single-flight).
    pending: Arc<Mutex<Option<Result<ChatMsg, String>>>>,
}

impl Default for ConsolePane {
    fn default() -> Self {
        let base =
            std::env::var("AETHER_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".into());
        Self {
            base_url: base.trim_end_matches('/').to_string(),
            organism: 1,
            run: 0,
            app: 1,
            input: String::new(),
            messages: vec![ChatMsg {
                role: ChatRole::System,
                text: "Coprocessador (não chatbot): cite/act com evidência, senão abstain.\n\
Ex. que funciona: «qual a capital do Brasil?»\n\
«oi» / prosa livre → abstain (OOV). API: cargo run --release --bin aether-api --features api\n\
SGDB: defina AETHER_SGDB_STDIO/.env para citar memória em OOV."
                    .into(),
                verdict: String::new(),
            }],
            busy: false,
            pending: Arc::new(Mutex::new(None)),
        }
    }
}

impl ConsolePane {
    /// Consome resposta do worker HTTP (chamar a cada frame).
    pub fn poll(&mut self) {
        let mut slot = match self.pending.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let Some(res) = slot.take() {
            self.busy = false;
            match res {
                Ok(msg) => self.messages.push(msg),
                Err(e) => self.messages.push(ChatMsg {
                    role: ChatRole::System,
                    text: e,
                    verdict: "error".into(),
                }),
            }
        }
    }

    /// Dispara pergunta (single-flight).
    pub fn submit(&mut self) {
        let q = self.input.trim().to_string();
        if q.is_empty() || self.busy {
            return;
        }
        self.input.clear();
        self.messages.push(ChatMsg {
            role: ChatRole::User,
            text: q.clone(),
            verdict: String::new(),
        });
        self.busy = true;
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = json!({
            "model": "aether-phase-160",
            "messages": [{"role": "user", "content": q}],
            "aether": {
                "scope": {
                    "organism": self.organism,
                    "run": self.run,
                    "app": self.app
                }
            }
        })
        .to_string();
        let pending = Arc::clone(&self.pending);
        thread::spawn(move || {
            let out = match http_post_json(&url, &body) {
                Ok(raw) => Ok(parse_chat_reply(&raw)),
                Err(e) => Err(e),
            };
            if let Ok(mut g) = pending.lock() {
                *g = Some(out);
            }
        });
    }

    /// Limpa histórico (mantém aviso de sistema).
    pub fn clear(&mut self) {
        let tip = self.messages.first().cloned();
        self.messages.clear();
        if let Some(m) = tip {
            if m.role == ChatRole::System {
                self.messages.push(m);
            }
        }
    }
}

/// Desenha a aba Console IA.
pub fn draw_console(ui: &mut egui::Ui, pane: &mut ConsolePane) {
    pane.poll();

    ui.horizontal(|ui| {
        ui.label(RichText::new("Console IA").strong().size(22.0));
        ui.label(
            RichText::new("· cite / act / abstain · não é LLM")
                .size(16.0)
                .color(Color32::from_rgb(140, 150, 165)),
        );
    });
    ui.label(
        RichText::new(format!(
            "API {}  ·  escopo org:{} run:{} app:{}",
            pane.base_url, pane.organism, pane.run, pane.app
        ))
        .size(15.0)
        .color(Color32::from_rgb(120, 130, 145)),
    );
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label("URL");
        ui.add(
            TextEdit::singleline(&mut pane.base_url)
                .desired_width(220.0)
                .hint_text("http://127.0.0.1:8787"),
        );
        ui.label("org");
        ui.add(egui::DragValue::new(&mut pane.organism).speed(1));
        ui.label("run");
        ui.add(egui::DragValue::new(&mut pane.run).speed(1));
        ui.label("app");
        ui.add(egui::DragValue::new(&mut pane.app).speed(1));
        if ui.button("Limpar").clicked() {
            pane.clear();
        }
    });
    ui.add_space(4.0);

    let avail = ui.available_height().max(180.0);
    let input_h = 52.0;
    ScrollArea::vertical()
        .stick_to_bottom(true)
        .max_height((avail - input_h).max(120.0))
        .show(ui, |ui| {
            for m in &pane.messages {
                draw_bubble(ui, m);
                ui.add_space(6.0);
            }
            if pane.busy {
                ui.label(
                    RichText::new("AETHER pensando…")
                        .italics()
                        .color(Color32::from_rgb(120, 160, 180)),
                );
            }
        });

    ui.separator();
    ui.horizontal(|ui| {
        let te = ui.add(
            TextEdit::multiline(&mut pane.input)
                .desired_width(ui.available_width() - 100.0)
                .desired_rows(2)
                .hint_text("Pergunte um fato… (Enter envia; Shift+Enter quebra linha)"),
        );
        let enter =
            te.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
        let send = ui
            .add_enabled(
                !pane.busy && !pane.input.trim().is_empty(),
                egui::Button::new("Enviar"),
            )
            .clicked();
        if enter || send {
            pane.submit();
        }
    });
}

fn draw_bubble(ui: &mut egui::Ui, m: &ChatMsg) {
    let (who, color) = match m.role {
        ChatRole::User => ("Você", Color32::from_rgb(90, 140, 200)),
        ChatRole::Aether => ("AETHER", Color32::from_rgb(60, 170, 150)),
        ChatRole::System => ("Sistema", Color32::from_rgb(150, 120, 90)),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(who).strong().color(color));
        if !m.verdict.is_empty() {
            let vc = match m.verdict.as_str() {
                "cite" | "act" => Color32::from_rgb(80, 180, 120),
                "abstain" => Color32::from_rgb(200, 160, 70),
                "error" => Color32::from_rgb(200, 90, 90),
                _ => Color32::GRAY,
            };
            ui.label(
                RichText::new(format!("[{}]", m.verdict))
                    .size(15.0)
                    .color(vc),
            );
        }
    });
    ui.add(
        egui::Label::new(
            RichText::new(&m.text)
                .size(17.0)
                .color(Color32::from_rgb(210, 215, 220)),
        )
        .wrap(),
    );
}

fn parse_chat_reply(raw: &str) -> ChatMsg {
    let v: Value = match serde_json::from_str(raw) {
        Ok(x) => x,
        Err(_) => {
            return ChatMsg {
                role: ChatRole::Aether,
                text: raw.chars().take(800).collect(),
                verdict: "error".into(),
            };
        }
    };
    if let Some(err) = v
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return ChatMsg {
            role: ChatRole::System,
            text: err.to_string(),
            verdict: "error".into(),
        };
    }
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let finish = v
        .pointer("/choices/0/finish_reason")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let xa = v.get("x_aether");
    let verdict = xa
        .and_then(|x| x.get("verdict"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if finish == "content_filter" || content.contains("\"abstain\":true") {
                "abstain".into()
            } else {
                "cite".into()
            }
        });
    let mut lines = Vec::new();
    lines.push(if content.is_empty() {
        "(sem content)".into()
    } else {
        content
    });
    if let Some(xa) = xa {
        if let Some(v) = xa.get("tnl_in_version").and_then(|k| k.as_str()) {
            lines.push(format!("tnl_in=v{v}"));
        }
        if let Some(c) = xa.get("parse_confidence").and_then(|c| c.as_f64()) {
            lines.push(format!("parse_conf={c:.2}"));
        }
        if let Some(i) = xa.get("parse_intent").and_then(|i| i.as_str()) {
            lines.push(format!("intent={i}"));
        }
        if let Some(cert) = xa.get("certified") {
            if let Some(auth) = cert.get("auth").and_then(|a| a.as_u64()) {
                lines.push(format!("utt_auth={auth}"));
            }
            if let Some(srcs) = cert.get("sources").and_then(|s| s.as_array()) {
                lines.push(format!("sources={}", srcs.len()));
            }
        }
        if let Some(k) = xa.get("learned_key").and_then(|k| k.as_str()) {
            lines.push(format!("learned={k} (≤120; HITL→255)"));
        }
        if let Some(k) = xa.get("key").and_then(|k| k.as_str()) {
            lines.push(format!("key={k}"));
        }
        if let Some(a) = xa.get("authority").and_then(|a| a.as_u64()) {
            lines.push(format!("authority={a}"));
        }
        if let Some(r) = xa.get("reason").and_then(|r| r.as_str()) {
            lines.push(format!("reason={r}"));
        }
    }
    ChatMsg {
        role: ChatRole::Aether,
        text: lines.join("\n"),
        verdict,
    }
}

/// POST JSON mínimo (localhost) sem dependência HTTP.
fn http_post_json(url: &str, body: &str) -> Result<String, String> {
    let (host, port, path) = parse_http_url(url)?;
    let mut stream = TcpStream::connect(format!("{host}:{port}")).map_err(|e| {
        format!(
            "API inacessível em {host}:{port} ({e}). \
Suba: cargo run --release --bin aether-api --features api"
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(20))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|e| format!("read: {e}"))?;
    let text = String::from_utf8_lossy(&buf);
    let Some(idx) = text.find("\r\n\r\n") else {
        return Err("resposta HTTP sem corpo".into());
    };
    let (head, body) = text.split_at(idx + 4);
    if !head.contains("200") && !head.contains("201") {
        let status = head.lines().next().unwrap_or("HTTP ?");
        return Err(format!(
            "{status}\n{}",
            body.chars().take(400).collect::<String>()
        ));
    }
    Ok(body.to_string())
}

fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
    let u = url.trim();
    let rest = u
        .strip_prefix("http://")
        .or_else(|| u.strip_prefix("https://"))
        .ok_or_else(|| "URL deve ser http://…".to_string())?;
    let (hostport, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".into()),
    };
    let (host, port) = if let Some((h, p)) = hostport.split_once(':') {
        (
            h.to_string(),
            p.parse::<u16>().map_err(|_| "porta inválida".to_string())?,
        )
    } else {
        (hostport.to_string(), 80)
    };
    Ok((host, port, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_default_path() {
        let (h, p, path) = parse_http_url("http://127.0.0.1:8787/v1/chat/completions").unwrap();
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 8787);
        assert_eq!(path, "/v1/chat/completions");
    }

    #[test]
    fn parse_abstain_content() {
        let raw = r#"{"choices":[{"message":{"content":"{\"abstain\":true,\"reason\":\"empty_scope\"}"},"finish_reason":"content_filter"}],"x_aether":{"abstain":true,"verdict":"abstain","reason":"empty_scope"}}"#;
        let m = parse_chat_reply(raw);
        assert_eq!(m.verdict, "abstain");
        assert!(m.text.contains("empty_scope") || m.text.contains("abstain"));
    }
}
