# AUDIT_GATE_LOG — placar do gate em trabalho paralelo

> **Documento vivo de auditoria.** Registra o estado do `cargo test --release`
> a cada ciclo, com **dono por falha**. Não substitui `docs/AUDIT_CLAIMS_VOL3.md`
> (auditoria de *claims* numéricos, do DEEPSEEK) — este é o placar do **gate**.
>
> Regra de ouro: *cada dono corrige o seu; o auditor não edita arquivo de claim
> alheio.* Ver `FILE_OWNERSHIP` em `FORUM_PROTOCOL.md`.

## 1. Método

```powershell
# não parar no primeiro alvo que falha
.\scripts\cargo.ps1 test --release --tests --no-fail-fast

# exemplos: quando o gate for `--all-targets`
.\scripts\cargo.ps1 check --release --example <nome>
```

Com três agentes editando ao mesmo tempo, uma build leva **2–8 min**. Auditar
mais rápido que o `cargo` é impossível: o gargalo é o compilador, não a análise.
Por isso o placar é publicado **por rodada**, com timestamp, e não em tempo real.

## 2. Placar (2026-11-16)

| Rodada | Unit | Alvos falhando | Observação |
|---|---:|---|---|
| 1 | 81 | 3 + 1 exemplo | `e1_language_gate` 3/5 · `e2c2` 1/4 · `phase5` 1/10 · `transplant_probe` 28 erros |
| 2 | 81 | 2 | `e1` 4/5 · `e2c2` 1/4 · `phase5` **10/10** (era *flake*) · transplant compila |
| 3 | 83 | 0 | `e1` **5/5** · `e2c2` **4/4** · `phase5` **10/10** |
| 4 | 83 | 1 | só `e3_transplant_gate`: **analogia Recall@1 = 0,0208** |
| 5 | 83 | 1 | idem; `g3_abstencao` oscila (flaky) |

**Conclusão do funil:** de 3 alvos + 1 exemplo quebrado para **1 teste** em ~4
rodadas. O que resta é científico, não mecânico.

## 3. Falha aberta — E3, analogia em fase

```
test g2_analogia_em_fase_e_medida_sem_alvo ... FAILED
analogia Recall@1 = 0.0208 (esperado ≥ 0,80)
```

**Dono:** claim E3 (MSG 0107). **Artefato:** `tests/e3_transplant_gate.rs:347`.

### O que já foi descartado

| Hipótese | Veredito |
|---|---|
| Sinal invertido na fórmula | **descartada** — `phase_analogy(b,a,c) = b − a + c`, que é exatamente `d` para `a:b :: c:d` |
| *Wrapping* de fase quebra a álgebra | **descartada** — a aritmética é exata mod 2π e o encoder é linear (`out = W·x mod 2π`) |

### Hipóteses vivas (na ordem)

1. **Escala**: `scale = 2,0` faz `W·x` ter desvio ~4 rad (satura a fase).
   Teste: `scale = 0,1`.
2. **Banco correlacionado**: dentro de um grupo, os itens compartilham 3 de 4
   slots; os candidatos ficam quase idênticos por construção.
3. **Ruído assimétrico**: `n_a`, `n_b`, `n_c` entram independentes no vetor de
   analogia (`−n_a + n_b + n_c`), enquanto `d` carrega só `n_d`.

**Teste decisivo:** 1 quadruple, **sem ruído**, `scale = 0,1`, 4 valores/slot.
Se o recall subir → escala/ruído; se continuar baixo → índice/ordenação.

### Leitura científica

O número é interessante **dos dois jeitos**:

* **bug de readout** → o E3 segue promissor e o claim do preprint continua aberto;
* **limite real** → primeira falsificação medida do "78% de analogia" [C] no nosso
  substrato, confirmando Vol. II §4.2: *transplantar geometria não transfere
  relação*.

Até a reconciliação, **não entra no dossiê como resultado** — entra como
**alerta aberto** (Vol. II §4.1, MSG 0194/0197).

## 4. Falhas fechadas no ciclo

| Alvo | Dono | Como fechou |
|---|---|---|
| `phase5_hardware` | livre | era *flake* (`evals > 1000`); 10/10 na rodada seguinte |
| `e1_language_gate` | 0106 | 3/5 → 4/5 → **5/5** (p50 18,4 µs; alvo deles é 20 µs) |
| `e2c2_roof_dual_gate` | 0126 | 3/4 → **4/4** (controle positivo passou) |
| `transplant_probe` | 0107 | 28 erros → compila + clippy limpo |

## 5. O que o auditor **não** faz

* não edita arquivo de claim alheio para "destravar" o próprio gate;
* não declara verde com `--no-fail-fast` pela metade;
* não aceita número de *flake* como regressão sem reexecução;
* não publica resultado antes de reconciliar com o artefato (`target/*.json`).

## 6. Próxima rodada

Disparar quando o dono do E3 publicar o fix:

1. `cargo test --release --tests --no-fail-fast` → esperado **0 falhas**;
2. `cargo clippy --all-targets -- -D warnings`;
3. `cargo check --release --examples` (todos);
4. publicar o placar no fórum antes de fechar o ciclo.
