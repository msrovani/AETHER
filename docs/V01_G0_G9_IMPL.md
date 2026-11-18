# G0–G9 — plano de implementação do freeze 0.1

Data: 2026-11-18. Crate hoje: `0.0.173`. Tag `v0.1.0` **não** entra neste plano.

Os IDs **G0–G9** são os do freeze de resposta (`cite|act|abstain`). O doc
[`V01_DELIVERY_PLAN.md`](V01_DELIVERY_PLAN.md) passou a chamar um subconjunto
de **F1–F7**. Este arquivo é a ordem de execução. Não cria gate novo e não
abaixa teto.

## Estado

| ID | Critério de pronto | Já existe | Falta |
|---|---|---|---|
| **G0** | `fmt --check`, `clippy -D warnings`, `cargo test --release` da suite default | scripts parciais | uma rodada limpa, sem lock de `cargo` |
| **G1** | ACQ ≥ 99, 12/12 `MEASURED`, `hot_path_bytes = 0` | `tests/cognitive_audit.rs` | re-rodar e copiar o scorecard para o close |
| **G2** | `api_eval`: exact+ruído 100% hit; paráfrase, caixa, 1 caractere, prefixo e escopo 100% abstain; false-act = 0 | `tests/api_eval.rs` | revalidar em `--release` |
| **G3** | `agent_eval_suite` ≥ 24/24, `false_act = 0`, `language_port_gate` 9/9 | os dois testes | revalidar |
| **G4** | `api_integrity` + `api_lifecycle` + `agent_redteam_gate` 4/4 | os três testes | revalidar |
| **G5** | forget 255 sem store → 500; `VerifiedFact` → 200; 255 só com HITL | `api_agent` + `agent_certificate_authority` | revalidar os filtros |
| **G6** | `agent_undo_sgdb_gate` + stress verdes | os testes | revalidar |
| **G7** | `usability_gate` cobre U2–U5, U7–U11, U13, **ou** a tabela 1:1 em `USABILITY_TESTS.md` | agregador + tabela | a tabela ainda diz que o empacote falta; alinhar o texto ao arquivo e rodar o teste |
| **G8** | API sobe, fato conhecido cita, desconhecido abstem, em &lt; 15 min, com JSON | `scripts/v01_smoke.ps1` passou 7/7 | o wrapper F7 (`v01_freeze_smoke.ps1`) ainda não fechou: o log parou no lock do `cargo` |
| **G9** | `Cargo.toml` `0.1.0`, CHANGELOG com a quebra de semântica, tag anotada | checklist vazio | **só depois de G0–G8 e de pedido humano explícito** |

Nada disso está preenchido em [`CYCLE_CLOSE_V01_FREEZE.md`](CYCLE_CLOSE_V01_FREEZE.md).

## Mapa G → F

| G | F correspondente | Comando |
|---|---|---|
| G3 | F1 | `cargo test --release --features agent --test agent_eval_suite` |
| G3 | (language) | `cargo test --release --features agent --test language_port_gate` |
| — | F2 | `cargo test --release --test cic_abstain_gate` |
| G2 + parte de G4 | F3 | `cargo test --release --features api --test api_eval --test api_agent --test api_integrity` |
| G4 + G7 | F4 | `cargo test --release --features agent --test usability_gate --test agent_redteam_gate` |
| G1 | F5 | `cargo test --release --test cognitive_audit` |
| G0 | F6 | `cargo fmt --all --check` e `cargo clippy --all-targets --features "api,agent" -- -D warnings` e `cargo test --release --lib` |
| G5 | (dentro de F3/F4) | filtros `forget` em `api_agent` / `api_lifecycle` e `agent_certificate_authority` |
| G6 | (não está em F1–F7) | `cargo test --release --features agent --test agent_undo_sgdb_gate` |
| G8 | F7 | `powershell -NoProfile -File scripts/v01_freeze_smoke.ps1` |
| G9 | onda V0.1-D | **FEITO** bump + tag `v0.1.0` · MSG 0666 · `c8621d2` |

`scripts/v01_freeze_gates.ps1` **não** é G0–G9. Ele só cobre fmt, `check --lib`,
`usability_gate`, `language_port_gate` e o smoke. Não substitui a tabela acima.

## Ordem

Um `cargo` por vez. `CARGO_TARGET_DIR` único. Host sem o embed D1/R5.

1. **G0a.** `cargo fmt --all --check`. Se falhar, fmt só nos arquivos deste freeze, não nos de outro dono.
2. **G0b.** `clippy -D warnings` com features `api,agent`. Corrigir só o que este comando apontar.
3. **G3, G2, G4, G5, G6, G7** numa invocação só, com `--features "api,agent"`, testes nomeados na tabela. Um vermelho para o dono do arquivo; não reescrever o teste para ficar verde.
4. **G1.** `cognitive_audit` em `--release`. Copiar ACQ, 12/12 e `hot_path_bytes` para o close.
5. **G0c.** `cargo test --release --lib` depois que os testes de integração acima já compilaram.
6. **G8.** `v01_freeze_smoke.ps1` com o `aether-api.exe` já buildado. Não disparar outro `cargo build` em paralelo.
7. Preencher `CYCLE_CLOSE_V01_FREEZE.md` com exit code, contagem e caminho do artefato. G9 permanece desmarcado.

## Artefato

Cada passo grava uma linha em `target/v01_freeze/g0_g9.json`:

```json
{"id":"G2","ok":true,"exit":0,"detail":"api_eval exact+abstain"}
```

Sem esse arquivo completo, o freeze não está medido — mesmo que um teste tenha passado em outra sessão.

## Fora deste plano

D1 ColBERT, BGE-M3, NysHD, embed de produção, FFT-G2, MSI, MCTS (pesquisa).
Tag `v0.1.0` **já publicada** (MSG 0666 / `c8621d2`) — fora deste plano de G0–G8, não do repo.
O piso de abstenção 0,60 e o teto de AUC do mapa não se mexem para “passar” E1.
