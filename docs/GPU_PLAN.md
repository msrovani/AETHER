# GPU no AETHER — plano de admissão e implementação

**Premissa medida:** o laço de 60 Hz é *latency-bound* e bit-exato; GPU é
*throughput-bound* e não determinística na ordem de redução. Portanto GPU só
entra onde (a) o trabalho por transferência é grande, (b) a projeção cabe no
alvo e (c) a razão medida ≥ 2× — **medida, não projetada**. O que não passa
fica em CPU por decisão, não por falta de tempo.

Este plano usa o vocabulário do `gpu_gate` (`src/core/gpu_gate.rs`): cada
workload recebe `act` (vai para GPU), `cite` (medir de novo), `propose`
(protótipo fora do default) ou `abstain` (fica CPU). **Nada entra no default.**

---

## Critério de admissão (as quatro condições)

Um workload só vira feature `gpu` se **todas** valerem:

1. **Trabalho por transferência** ≥ 10⁷ flops (abaixo disso o *launch*/sync de
   5–20 µs domina — é o que o `gpu_gate` já calcula com 12 B/aresta/passo,
   50 GB/s CPU vs 300 GB/s GPU e piso de ~200 µs);
2. **Roofline**: a projeção GPU cabe no alvo de latência do estágio;
3. **Razão medida ≥ 2×** no mesmo workload, no hardware de referência;
4. **Fallback CPU** que passa os mesmos gates (ausência de GPU nunca é `panic`).

---

## Censo dos candidatos (números do próprio repositório)

| # | Workload | Tamanho | CPU hoje | Veredito esperado |
|---|---|---|---:|---|
| 1 | **JL do transplante** `project_into` (d=384 → 10.240 bits) | 3,93 M MACs/vetor ⇒ **3,93 GFLOP / 10³** e **393 GFLOP / 10⁵** | segundos a minutos | **`act`** — o caso mais forte do projeto |
| 2 | **FFT em lote** `WaveInterferenceBlock` L=8192 × B | L log L × B | ms | **`act`** se B ≥ 16 (já previsto em `HW_TARGET.md` §3) |
| 3 | **Boltzmann do dreamer** 10³–10⁴ caminhos independentes | embaraçosamente paralelo | s | **`propose`** (throughput, não latência) |
| 4 | **Sweep Ising em lote** (E2c/E2c‑2 × 10⁴ instâncias) | ~10⁵ flops/instância | minutos | **`propose`** com gate **estatístico** |
| 5 | `record` do T0, M5 (D=160), solver do E2 | 1,28 KB · 25,6k flops · 10⁵ flops | 0,2 µs · µs · 135 µs | **`abstain`** — GPU perde |
| 6 | Conversão "em tempo real" de outro modelo | encoder ~10⁹ flops/token | — | **fora do AETHER**: GPU no *encoder*; a costura é 1,28 KB |

---

## Fase G0 — censo medido, **sem compilar `wgpu`** (1 sessão)

**Construir** `examples/gpu_census.rs`:

* mede, com `Instant` + alocador contador, o custo real de:
  * `JlProjector::project_into` com `n_rows ∈ {10³, 10⁴, 10⁵}` (`d=384`, `bits=10_240`);
  * `WaveInterferenceBlock::forward_into` com `L ∈ {512, 2048, 8192}` × `B ∈ {4,16,64}`;
  * `ThermodynamicDreamer` com 10³–10⁴ caminhos;
  * lote de instâncias Ising `n=20` × 10⁴;
* converte cada medida em `gpu_gate::Workload::new(n, steps, target_us)` e
  imprime o veredito **calculado** (não o da doc);
* grava `target/gpu_census.json` com GFLOP/s e GB/s efetivos.

**Testado**: `tests/gpu_census_gate.rs` (roda sempre, sem feature) — os números
de G0 precisam ser reprodutíveis dentro de 20% e o veredito do JL tem de ser
`WorthIt` (senão o item 1 sai do plano e a GPU morre por evidência).

**Saída**: decisão `act|propose|abstain` por workload, com número.

---

## Fase G1 — GEMM do transplante atrás de `--features gpu` (2–3 sessões)

**Construir**

* `Cargo.toml`: `gpu = ["dep:wgpu"]`, `wgpu` **opcional** (fora do `default`);
* `src/core/gpu/mod.rs`: `GpuContext` (probe de adapter, `AETHER_GPU=0` desliga,
  aviso de telemetria quando não há device);
* `src/core/gpu/jl.rs`: kernel do JL — codebook ±1 empacotado (384×10.240 bits =
  491 KB), *upload* uma vez por sessão, dispatch por lote, readback de `words`;
* `JlProjector::project_into`: dispatch `#[cfg(feature = "gpu")]` quando
  `n_rows ≥ 1024` **e** existe device; senão CPU;
* política: **a decisão de sinal é a mesma** (soma de ±x por bit ⇒ idêntica em
  qualquer ordem que preserve o sinal; usar acumulador inteiro/f64 conforme o
  caso para manter igualdade bit-a-bit).

**Testado** (`tests/gpu_jl_gate.rs`, `#[cfg(feature = "gpu")]`, mais um teste
CPU-only que roda sempre):

1. **equivalência**: `project_into` GPU == CPU **bit-a-bit** em 10³ vetores
   (JL é soma de ±x — não há motivo para divergir; se divergir, o kernel está
   errado, não a GPU);
2. **razão ≥ 2×** em 10⁴ vetores (medida, não projetada);
3. **fallback**: com `AETHER_GPU=0` o caminho é CPU e o resultado é idêntico;
4. **sem device**: `GpuContext::probe()` → `None` e `project_into` segue em CPU
   **sem panic** (simulado por env var, para rodar em CI sem GPU).

**CI**: `cargo test` (sem feature) continua sendo o gate obrigatório; o job com
`--features gpu` roda só onde há adapter.

---

## Fase G2 — FFT em lote (1–2 sessões)

**Construir**: kernel de FFT (compute shader) para `L=8192`, `B ≥ 16`, atrás da
mesma feature; `rustfft` continua o default do organismo.

**Testado** (`tests/gpu_fft_gate.rs`):

* erro relativo ≤ 1e‑4 contra CPU (FFT **não** é bit-exata — gate por tolerância,
  declarado);
* razão ≥ 2× em `L=8192, B=64`;
* **invariante estrutural**: o organismo (`simulation::closed_loop_organism`)
  não referencia `core::gpu` — teste que varre o módulo (`include_str!` + grep)
  e falha se aparecer dependência fora de `#[cfg(feature = "gpu")]`.

---

## Fase G3 — sweeps Ising em lote (2–3 sessões, opcional)

**Construir**: bSB/roof‑dual **batelado** (milhares de instâncias independentes
num dispatch) para E2c/E2c‑2 em lote; só para *experimento*, nunca para o
pipeline por instância.

**Testado** (`tests/gpu_ising_sweep_gate.rs`) — **estatístico**:

* mesma taxa G1/G2 (100% SAT, precisão 100%) em 200 instâncias;
* `|ΔE| ≤ 1e‑3` e teste KS sobre a distribuição de ótimos vs CPU;
* declarar no doc: **o `digest` do E2 e o `bsb_compacto_igual_ao_solver_auditado`
  não valem no caminho GPU** (bit-a-bit morre por construção).

---

## Invariantes que este plano **não** pode quebrar

| Invariante | Como fica garantido |
|---|---|
| `record` no T0 ≤ 4,07 µs e **0 alloc** | módulo gpu não é referenciado pelo F1 |
| `t0_ledger_calls = 0` | idem |
| 60 Hz do organismo sem dependência gráfica | teste estrutural da G2 |
| gates bit-exatos do E2 (`digest 0x6a49e4…`) e do BSB denso↔esparso | GPU só no caminho de *lote*, fora deles |
| fallback obrigatório, nunca `panic` | teste de fallback em cada fase |
| crate sem `gpu` funciona completo | `gpu` fora do `default`; CI sem feature |

---

## Riscos e mitigação

| Risco | Mitigação |
|---|---|
| Máquina de referência é notebook com iGPU (Vulkan/DX12 pode não passar o gate) | G0 mede; se não passar, o plano morre em `abstain` — e isso é resultado |
| Não-determinismo de redução | G1 exige bit-a-bit (JL é soma de ±x); G2/G3 usam tolerância declarada |
| Dois caminhos para manter | kernel pequeno e isolado em `core/gpu/`, com fallback testado |
| `wgpu` no toolchain (Rust 1.100) | fixar versão no `Cargo.lock` e testar build sem feature |
| Custo de CI sem GPU | job `gpu` opt-in; suíte padrão intacta |

---

## Ordem e esforço

```
G0 (medir)  → G1 (JL)  → G2 (FFT)  → G3 (sweep ising, opcional)
1 sessão      2–3          1–2          2–3
```

**Recomendação:** executar G0 e G1. Se G0 mostrar `WorthIt` no JL (esperado:
3,93 GFLOP em 10³ vetores), a GPU passa a ter um caso de uso **real, offline e
sem tocar o laço** — que é o único tipo que este projeto aceita.

**Artefatos previstos:** `examples/gpu_census.rs`, `tests/gpu_census_gate.rs`,
`target/gpu_census.json`, `src/core/gpu/{mod,jl}.rs`, `tests/gpu_jl_gate.rs`,
feature `gpu` no `Cargo.toml`, e uma linha em `docs/HW_TARGET.md` §3 dizendo qual
workload foi admitido (com o número medido).
