# Release 0.6.1 — A4 T-CLIP: clipboard sem shell livre

**Data:** 2026-11-24
**Âncora:** após 0.6.0 (A1 T-TAB, A2 T-EVAL, A3 T-P2P).
**Natureza:** código + gate + docs (fecha A4 de `docs/PLANO_A1_D4.md`).

## O que muda

| Tema | Item | Entrega |
|---|---|---|
| **A4 T-CLIP** | `src/agent/clip.rs` | `ClipBackend` com stdin default; `OsBackend` sob `--features clip` (`arboard 3.7`); redact de segredo forte -> deny; hex32 contextual -> redação; diff -> `code.patch` sob sandbox; saída só por `CertifiedUtterance` |

Gate novo (vermelho->verde, artefato em `target/`):

- `tests/agent_clip_gate.rs` -> `target/aether_clip_gate.json` (8 checks:
  stdin default + limites, 6 classes de redação, deny de segredo, sem falso
  positivo grosseiro, `code.patch` sob sandbox, sem escape, COH antes da
  escrita, sem processo/shell no fonte).

`tools/cycle_gate.py` passa a rodar o gate A4 — o ciclo fecha em 10 checks.

## Correções de causa

- **Certificar antes de escrever.** `process` chamava `ingest` (que grava
  `code.patch`) e só depois `certify_utterance_template`. Com claims vazias o
  retorno era `Coherence`, mas o arquivo já estava na sandbox — lote negado com
  efeito colateral. A ordem foi invertida e o gate prova com diff + claims
  vazias que a sandbox fica limpa (`saida_pos_coh`).
- **hex32 não é prova de segredo.** A regra antiga negava o lote inteiro para
  qualquer 32 caracteres hex — md5, uuid sem hífen e hash truncado são comuns em
  diff e custavam o paste de até 256 KiB, sem escape. Agora hex32 só conta com
  contexto de atribuição (`key`, `token`, `secret`, `password`, `passwd`,
  `auth`) e, nesse caso, é redigido e o lote segue.
- **`redact` deixou de ser calculado e descartado.** `ingest` montava
  `redaction.text` e usava apenas as classes. O texto aceito, gravado em
  `code.patch` e devolvido no payload é o redigido.
- **Cobertura real:** JWT (`eyJ…`, 2 segmentos, >= 40 bytes) entrou como classe
  forte — bearer token passava limpo.

## Revisão e corte

`src/agent/clip.rs` 502 -> 473 linhas; `tests/agent_clip_gate.rs` 337 -> 309.

Removidos por não terem consumidor: `CLIP_AUTH`, `Finding` (offsets nunca
lidos), `contains_secret`, `ClipKind::label`, `ClipPayload.bytes`,
`ClipBackend::label` e `ClipDrop.backend` (só ecoavam string fixa no artefato).
Três `scan_prefixed` viraram uma tabela `PREFIXES`. O check `sem_shell` perdeu o
filtro de comentários: o doc do módulo não cita mais os needles proibidos, então
a varredura é literal (10 linhas) e continua guardando o contrato.

## Compatibilidade

- Dependência nova **opcional**: `arboard 3.7` com `default-features = false`
  (com defaults o resolver offline falha em `tiff`); só compila sob
  `--features clip`. `cargo check --all-targets --offline` limpo.
- `src/knowledge/sgdb/` e `src/core/persistency_search.rs` intocados.
- Tetos mantidos: auth <= 120, A3 recall >= 0,90, F4 hit@5 >= 0,974.

## Limites honestos

- O clipboard real do SO **não foi exercitado**: `OsBackend` só foi compilado
  sob a feature, nunca aberto contra um clipboard vivo.
- Detecção não cobre base64 genérico, `.env` com valor sem prefixo reconhecível
  nem JWT com header fora de `eyJ`.
- `aether-agent ingest` ainda **não existe**: a A4 é biblioteca + gate, sem
  chamador de CLI. O caminho útil (stdin + `--file`) fica para a próxima fase.

## Evidência

- `target/aether_clip_gate.json` — `ok=true`, 8/8 checks.
- `data/cycle_gate.json` — ciclo reexecutado no tree final com 0.6.1: 10/10.
- Vermelho->verde da ordem de efeitos: com o código antigo o gate reporta
  `saida_pos_coh: lote sem COH gravou code.patch antes de certificar`.
