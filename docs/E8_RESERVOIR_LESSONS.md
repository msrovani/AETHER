# E8 — lições de reservoir computing (reservatório congelado)

**Contexto:** Vol. II §6/§10 — “é preciso treinar a física do campo ou só o
readout linear?”. O E8 responde com benchmark, não com opinião.
**Arquivos:** `src/core/reservoir.rs`, `tests/e8_reservoir_gate.rs`,
`examples/reservoir_readout_bench.rs`.
**Artefato:** `target/aether_e8_reservoir.json`.

---

## 1. O que foi medido

| Tarefa / braço | NRMSE | Treino | Inferência |
|---|---|---|---|
| Mackey–Glass τ=17, 10⁶ | **2,33 × 10⁻⁵** | **1,57 s** | **1.733.418 pred/s** |
| Mackey–Glass, 5 sementes (100k) | **2,00 × 10⁻⁵ ± 2,35 × 10⁻⁶** | — | — |
| NARMA-10, 16k | **0,1286** | 0,24 s | 237.044 pred/s |
| RLS online (MG 20k) | 0,00655 | 0,20 s | — |
| Busca conjunta 40 passadas | 1,31 × 10⁻⁴ | **39,6–45,7× mais lenta** | — |

Gates de release (`tests/e8_reservoir_gate.rs`): **3/3 PASS** —
NRMSE MG `< 10⁻³`, treino `10⁶ < 60 s`, NARMA `< 0,15`, speedup do congelado
`≥ 20×` e regressão `≤ 5%`.

---

## 2. Referências que mudaram o desenho

* Lukoševičius & Jaeger, *A tutorial on reservoir computing* (2009).
* Lukoševičius, *A practical guide to applying echo state networks* (2012).
* Jaeger, *The “echo state” approach to analysing and training recurrent
  neural networks* (2001).
* ReservoirPy / EchoBay — faixas de `ρ`, `α`, `β`.
* NGRC, *On the emergence of numerical instabilities in Next Generation
  Reservoir Computing* (2025) — Cholesky/SVD e condicionamento.
* SPIRES RLS — RLS online é `O(N²)` por passo e sensível a `P`.

Regras práticas adotadas:

1. `ρ(W) ≈ 1,0` (borda do caos), `α` casando com a escala temporal da série.
2. Esparsidade de 5–12 conexões por nó.
3. Readout por **equações normais + Cholesky**, com ridge.
4. Washout dos primeiros estados.
5. `β` varrido no mesmo `XᵀX` (não reexecutar o reservatório).

---

## 3. Arquitetura final

* `Reservoir` — matriz esparsa `n × degree`, escala por iteração de potência,
  estado `x_{t+1} = (1−α)x_t + α·tanh(Wx_t + w_in u_t)`; passo `O(n·k)`.
* `RidgeReadout` — acumula `XᵀX` e `Xᵀy` em `f64`; resolve
  `(XᵀX + λT·I)w = Xᵀy` por Cholesky com jitter progressivo.
* `RlsReadout` — Sherman–Morrison com esquecimento; mantido para regime
  não-estacionário (T03), não para treino em lote.
* `FrozenModel` — física congelada; `recalibrate(λ)` reusa o acumulador.
* `RlsModel` — variante online.
* `joint_search` — 40 candidatos `(ρ, α, input_scale)`, cada um com uma passada
  completa; baseline sem gradiente.

---

## 4. Lições (o que custou tempo)

1. **Mackey–Glass: τ é tempo, não passos.** Com `dt=0,1`, `τ=17` exige atraso de
   **170 amostras** (`τ/dt`). Usar 17 passos estabiliza o ponto fixo `x*=1` e a
   série vira constante — o benchmark passa a medir nada.
2. **História inicial não constante.** Mesmo com o atraso certo, história
   uniforme pode amortecer o atrator; inicialize com um padrão variado.
3. **NARMA-10 é supervisionado.** O reservatório deve ser dirigido por `u[t]`,
   não por `y[t]`; `targets[t] = y[t+1]`. Alimentar `y` no reservatório derruba
   o NRMSE para ~0,3.
4. **RLS em lote é frágil.** O `P` infla com esquecimento e o condicionamento
   piora; ridge/Cholesky é ~265× mais preciso e ~12× mais rápido no mesmo
   trecho. RLS fica para online.
5. **λ importa e é barato.** `1e-10 → 2,45e-5`; `1e-8 → 1,26e-4`;
   `1e-6 → 1,22e-3`. Varra no mesmo `XᵀX`.
6. **Meça inferência sem treinar.** O loop antigo chamava `observe` durante a
   avaliação; isso mistura treino e inferência e subestima o throughput.
7. **`fast_tanh` (Padé)** remove a chamada transcendental do passo: +47% de
   throughput sem mudar o NRMSE no regime saturado.
8. **Esparsidade é trade-off mensurável.** 10/nó: 0,154 s / 2,32e-5; denso:
   0,576 s / 1,14e-5 → 3,75× mais rápido por 2× de erro (ambos irrelevantes
   para o gate de 1e-2).
9. **Baseline conjunto tem de pagar o preço.** Comparar readout contra “Oja
   local” dava 2,9×; 40 passadas completas de busca dão **39,6–45,7×**, que é o
   custo real de ajustar a dinâmica sem backprop.
10. **Timing depende do host.** Em máquina carregada o throughput cai 10×; o
    gate usa piso conservador e o número “de vitrine” é medido em release limpo.

---

## 5. Reprodução

```powershell
cargo test --release --test e8_reservoir_gate -- --nocapture
cargo run  --release --example reservoir_readout_bench
```

O exemplo escreve `target/aether_e8_reservoir.json` com: MG, 5 sementes,
ablação de esparsidade, varredura de λ, RLS, busca conjunta e NARMA.

---

## 6. Limitações declaradas

* O “40×” é contra **40 passadas de busca sem gradiente**, não contra
  backprop do campo; o invariante `0 backprop` continua.
* NARMA-10 em 0,1286 é competitivo, não SOTA; o objetivo do E8 é o paradigma
  (readout congelado vs dinâmica ajustada), não bater um leaderboard.
* O throughput absoluto depende de carga da máquina; o gate é conservador.
* A configuração foi escolhida por validação; o teste usa um trecho separado.
