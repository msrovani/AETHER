# FORUM_PROTOCOL — canal textual UTF-8 (AETHER)

**Verdade operacional:** `C:\Users\msrov\Desktop\FORUM.txt` é o único canal de
deliberação entre agentes. Formato definido por **MSG 0001** (`FORUM_PROTOCOL_V1`):
arquivo **append-only**, **UTF-8**, um bloco `===MSG N===` … `===END N===` por
mensagem. A rede UDP dos nós continua binária (40 B); o fórum de análise é **texto**.

> **Deprecação:** o protocolo binário hex (`magic 0xA3`, CRC32, opcodes) e a
> escrita hex via `forum.py send|init|propose` estão **obsoletos**. Não use
> hex no `FORUM.txt`. Histórico binário, se existir em backups, não é a fonte
> de verdade atual.

## 1. Ferramentas

| Ferramenta | Papel |
|---|---|
| `tools/forum_post.ps1` | **Escrita canônica (Windows)** — lock exclusivo, atribui próximo `MSG_ID`, append UTF-8 |
| `python tools/forum_post.py` | **Escrita canônica (Linux/Cloud)** — mesmo contrato; path via `FORUM`/`FORUM_PATH`/`AETHER_FORUM` ou `./FORUM.txt` |
| `python tools/forum.py verify` | Valida gramática textual + detecta `MSG_ID` duplicados (UTF-8; fallback cp1252 se legado) |
| `python tools/forum.py dump` | Lista mensagens (só leitura) |
| `python tools/forum.py next-id` | Mostra o próximo `MSG_ID` livre (max+1) |

**Cloud Agent:** o Desktop Windows (`C:\Users\msrov\Desktop\FORUM.txt`) não
existe neste VM. Espelhe o arquivo em `/workspace/FORUM.txt` (ou exporte
`FORUM_PATH`) antes de `dump`/`post`. A verdade operacional continua sendo o
arquivo do Desktop; o espelho é só ponte de leitura/escrita.

```powershell
cd C:\Users\msrov\Documents\AETHER
powershell -NoProfile -ExecutionPolicy Bypass -File tools/forum_post.ps1 `
  -From CURSOR -Type RELATORIO -Topic MEU_TOPICO `
  -Body "CHAVE=valor","OUTRA=valor"
python tools/forum.py verify
python tools/forum.py dump --last 5
```

## 2. Gramática (MSG 0001)

```
===MSG NNNN===
FROM: AGENTE
TYPE: PROPOSTA|VOTO|CONFIRMACAO|RELATORIO|QUERY|BLOQUEIO|ACK|STATUS|CLAIM|ALERTA|...
TOPIC: TOKEN_CURTO
TO: TODOS
DEADLINE: - | timestamp
DATA:
  CHAVE=valor_curto_ou_token
  ...
===END NNNN===
```

* `KEYS_UPPERCASE=SIM`, um fato por linha sob `DATA:`
* Prosa proibida (máx. 2 linhas de comentário); limite ~40 linhas/post
* `MSG_ID` = sequencial global atribuído pelo escritor (preferir `forum_post.ps1`)
* `FILE_OWNERSHIP` = 1 arquivo / 1 agente (sem colisão de escrita)
* Append-only: **nunca** editar nem apagar blocos antigos

## 3. Desambiguação de `MSG_ID` duplicados

Colisões históricas (ex.: 0006, 0007, 0013–0018, 0040, 0053) **permanecem** no
arquivo (append-only). Regra de leitura:

1. Citar sempre `MSG_ID` + `FROM` + `TOPIC` (e, se ambíguo, posição/`INDEX` no arquivo).
2. `forum.py verify` reporta duplicatas sem falhar o parse UTF-8.
3. Novos posts usam `max(MSG_ID)+1` via `forum_post.ps1` — sem renumeração destrutiva.

## 4. Regras operacionais

1. **append_only** — correção = MSG nova (errata/`ref`), nunca rewrite.
2. **claim_before_code** — claim no fórum antes de editar arquivo alheio.
3. **quorum** — propostas passam com quórum (MSG 0001); empate → ORC-0.
4. **tests_release** — módulo `done` só com `cargo test --release` verde (quando o gate se aplica).
5. **human_arbiter** — conflito sem quórum sobe para o humano.

## 5. Missões (legado MSG 0002)

| id | módulo | entrega |
|---|---|---|
| m0 | scaffold | `Cargo.toml`, `lib.rs`, esqueleto |
| m1 | `thermodynamic_solver` | bSB batch |
| m2 | `wave_interference` | Fourier block |
| m3 | `kuramoto_field` | UDP 40 B |
| m4 | `closed_loop_organism` | integração + TUI |
| m5–m8 | fase 2 | memória / sonhador / morfogênese / daemon |

Lease típico: 900 s renovável por STATUS/CLAIM.

## 6. Build lease (2026-11-16)

Quatro janelas de build vermelho na mesma tarde vieram de edição concorrente em
`src/core/phase_language.rs` e de refatoração de arquivo sob claim alheio. Regras:

1. **Check antes de sair.** Todo agente roda `cargo check --lib` (ou
   `cargo test --no-run`) antes de encerrar o turno. Build vermelho bloqueia todos.
2. **Arquivo sob claim alheio.** Se a mudança toca um arquivo que outro agente
   reivindicou, poste `ACORDO`/`STATUS` **antes**. Não refatore o arquivo do outro
   sem ACK explícito; melhoria de performance não dispensa o ACK.
3. **Janela curta.** Mudança de assinatura pública entra com o build verde na
   mesma janela. Se não der, reverta e poste `BLOQUEIO` com o erro exato.
4. **Docs: um escritor por tabela.** Linhas de registro (Vol. III §11/§12) são
   adicionadas por **append de linha**, nunca por reescrita da tabela. Se a âncora
   sumiu, poste `ERRATA` e reaplique.
5. **Encoding.** Nunca editar `.rs`/`.md` com `Set-Content`/`Get-Content` do
   PowerShell 5.1 (cp1252). Use ferramenta que preserve UTF-8; se corromper, a
   inversão determinística é `UTF8_decode(cp1252_encode(texto))`.
