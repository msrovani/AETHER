# FORUM_PROTOCOL — protocolo binário dos 3 nós (AETHER v1)

`C:\Users\msrov\Desktop\FORUM.txt` é o único canal de deliberação entre os três agentes.
Ele **não contém texto**: cada linha é um frame binário completo em hexadecimal maiúsculo.

## 1. Formato do frame

| offset | bytes | campo | conteúdo |
|---|---|---|---|
| 0 | 1 | `magic` | `0xA3` |
| 1 | 1 | `version` | `0x01` |
| 2 | 2 | `seq` | uint16 BE — estritamente crescente no arquivo inteiro |
| 4 | 1 | `src` | nó: `1`=buffy/freebuff, `2`=cursor, `3`=opencode |
| 5 | 1 | `dst` | `0`=broadcast, senão o id do destinatário |
| 6 | 1 | `type` | opcode (tabela abaixo) |
| 7 | 2 | `ref` | uint16 BE — seq do frame respondido (`0` = nenhum) |
| 9 | 2 | `len` | uint16 BE — tamanho do payload |
| 11 | `len` | `payload` | depende do tipo |
| 11+len | 4 | `crc32` | IEEE, calculado sobre `version..fim do payload` |

Frame = `15 + len` bytes. Uma linha = `frame.hex().upper()`.

## 2. Opcodes

| code | nome | payload |
|---|---|---|
| `0x01` | ANNOUNCE | `caps:u32 BE` + nome (≤16 bytes) |
| `0x02` | DICT | sequência `[id:u8][len:u8][ascii]` — vocabulário in-band |
| `0x03` | CLAIM | `mission:u8 prio:u8 eta:u16 ttl:u16 flags:u8` |
| `0x04` | PROPOSE | `proposal_id:u8` + fluxo de tokens |
| `0x05` | VOTE | `target_seq:u16 vote:u8(0 abst/1 sim/2 não) reason:token:u8` |
| `0x06` | STATUS | `mission:u8 state:u8 pct:u8` |
| `0x07` | COMMIT | `mission:u8` + 8 bytes de sha |
| `0x08` | ALERT | `code:u16 mission:u8` |
| `0x09` | RULE | `rule:u8` + params |
| `0x0A` | DONE | `mission:u8` + 8 bytes de sha |

Missões: `0`=m0_scaffold, `1`=m1_thermo, `2`=m2_wave, `3`=m3_kuramoto, `4`=m4_organism.
Estados: `0`=proposed, `1`=claimed, `2`=in_progress, `3`=review, `4`=done, `5`=blocked.

## 3. Tokens

PROPOSE não carrega texto: carrega **ids** do vocabulário publicado no frame DICT (`0x02`).
Escapes: `0xFE` + 1 byte = inteiro 0..255; `0xFD` + 2 bytes BE = inteiro até 65535; `0xFF` = quebra de cláusula.

O vocabulário é **apenas acrescentável**: o DICT de bootstrap (seq 2) nunca é editado. Um novo token
entra como um novo frame DICT a partir do próximo id livre — e os três nós precisam da mesma ordem,
ou os ids divergem silenciosamente.

## 4. Ferramenta

```bash
cd C:\Users\msrov\Documents\AETHER
python tools/forum.py verify                 # magic + CRC + monotonicidade
python tools/forum.py dump                   # traduz para palavras (só leitura)
python tools/forum.py send --src 2 --type vote --payload 00 03 01 19 --ref 3
python tools/forum.py propose --src 3 --id 4 --tokens m2_wave ack review
```

Nenhum nó escreve hex à mão: o CRC e o `seq` são calculados pela ferramenta.

## 5. Regras propostas (sujeitas a voto, proposta p1 — seq 3)

1. **append_only** — ninguém edita nem apaga linhas; correção é um frame novo com `ref`.
2. **one_frame_per_line** + **hex_uppercase** — sem comentários, sem texto no arquivo.
3. **seq_monotonic** + **crc32** — validado por `forum.py verify` antes de cada escrita.
4. **no_edit_delete** — violação é ALERT para os outros dois nós e para o humano.
5. **claim_before_code** — só se implementa um módulo depois de CLAIM aceito e STATUS `in_progress`.
6. **vote_quorum_2of3** — propostas passam com 2 votos `yes` de 3 nós; empate não desempata sozinho.
7. **no_force_push** — nada de `git push --force`; integração só em `main` por merge revisado.
8. **tests_release** — cada módulo só é `done` com `cargo test --release` verde.
9. **binary_only / no_json / no_prose** — o canal é de máquina; explicação humana fica fora do fórum.
10. **human_arbiter / escalate_human** — conflito sem quórum sobe para o humano, não se resolve por força.

## 6. Missões (proposta p2 — seq 4)

| id | módulo | entrega | invariante |
|---|---|---|---|
| m0 | scaffold | `Cargo.toml`, `lib.rs`, perfis de build, esqueleto de pastas | `cargo build --release` |
| m1 | `thermodynamic_solver` | bSB batch com elitismo | 50 × N=64: H_final < H_inicial |
| m2 | `wave_interference` | Fourier block com `modes` fixos | L ∈ {128, 512, 2048} com os mesmos pesos |
| m3 | `kuramoto_field` | 4 nós UDP 40 B, thread+`mio` | R(t) > 0.95 em < 3 s |
| m4 | `closed_loop_organism` | integração + painel ratatui | quadro ≤ 8 ms, 0 alocações |

Cada módulo tem **um dono por lease de 900 s** (renovável por STATUS). Sem claim válido, o módulo é
de quem reivindicar primeiro com `seq` menor.
