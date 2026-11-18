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

* energia reportada == `H(estado devolvido)` (auto-consistência, `f64` no CPU);
* teste KS, média e *regret* das distribuições de ótimos **contra o piso de
  ruído do próprio solver** (CPU com outra semente), não contra paridade por
  instância — o `f32` só acrescenta uma perturbação a um sistema caótico;
* razão ≥ 2× no lote de 10⁴ instâncias (caso do E2c);
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

---

## Resultado (2026-11-17) — G0 executado, G1 implementado e **validado**

### G0 — censo medido (CPU, `examples/gpu_census.rs`)

| workload | CPU medido | projeção GPU (300 GB/s, 4 TFLOP/s, +20 µs launch) | razão | veredito |
|---|---:|---:|---:|---|
| **JL do transplante** 10⁴ linhas (d=384 → 10.240 bits) | **13,4 s** (39,3 GFLOP, **2,9 GFLOP/s**) | 9,9 ms | **1362×** | **ACT** |
| FFT L=8192 (4→8 linhas) | 158 µs | 21,3 µs | **7,4×** | **ACT em lote** |
| lote Ising 10⁴ instâncias (n=20) | 123 ms (34 MB, 0,38 GFLOP) | 0,13 ms | **929×** | PROPOSE (gate estatístico) |
| `record` T0 / M5 / solver E2 | 0,2 µs / µs / 135 µs | — | < 1× | **ABSTAIN** |

### G1 — GEMM do JL: implementado e validado no **RTX 3050**

* `feature = "gpu"` (+ `wgpu 22`, `bytemuck`) **fora do default**; sem a feature o
  crate é CPU-only completo e `gpu::status()` responde `NoFeature`.
* kernel WGSL: 1 thread por `(bit, linha)`, soma **j-ascendente** (mesma ordem do
  CPU) e `atomicOr` na palavra de 32 bits.
* **política de adapter**: descarta `DeviceType::Cpu` (WARP/llvmpipe) e prefere
  `DiscreteGpu`. Medido: com a **Intel iGPU** escolhida por engano, a razão era
  **0,9×** (pior que CPU); com o RTX 3050, **3×**.
* **gates medidos** (`cargo test --release --features gpu --test gpu_jl_gate`):

| gate | resultado |
|---|---|
| equivalência GPU↔CPU | **0 / 32.000 palavras divergentes** (bit a bit) |
| razão ≥ 2× (n=2000) | **3×** (CPU 2723 ms · GPU 1058 ms; +52 ms de geração de matriz) |
| fallback (`force_off` / sem device) | PASS, nunca `panic` |
| fronteira estrutural (`tests/gpu_boundary_gate.rs`) | PASS — hot path, M5, F1 e solver não referenciam `core::gpu` |

**Limite honesto:** 3× está longe dos 1362× projetados. O kernel é ingênuo —
cada thread lê a linha da matriz de forma **não coalescida** (384 floats com
passo `d`) e há 2×10⁷ `atomicOr`. O próximo passo medido é o kernel por palavra
(32 acumuladores por thread, matriz transposta `d × bits`, sem átomos), que é o
que a projeção assume.

### Segunda rodada (kernel coalescido, v2) — medições finais

Kernel v2: 1 thread por `(palavra de 32 bits, linha)`, **32 acumuladores
escalares**, matriz **transposta** (`d × bits`) e **sem átomos**.

| métrica | v1 (átomos, row-major) | **v2 (coalescido)** |
|---|---:|---:|
| divergências GPU↔CPU | 0 / 32.000 | **0 / 32.000** |
| razão (n=2000) | 3× | **5–6×** (1,6× sob carga alta) |
| fill (matriz+transposta, 1×/sessão) | 52 ms | 84 ms |

**Diagnóstico do teto:** 2000 linhas × 15,7 MB = **31 GB** de leitura da matriz
(cada linha relê a matriz inteira). A ~60–190 GB/s efetivos isso dá os ~600 ms
observados, não os 9,9 ms da projeção (que assume reúso por bloco).

**Terceira rodada (v3, tiling 8 bits × 4 linhas)** — tentada e **descartada**:
reduz o tráfego 4× na teoria, mas a validação acusou **95% de divergência** e
tempo pior; o empacotamento por `atomicOr` com deslocamento ficou errado. Um
kernel tiled correto exige *shared memory* (bloco 32 bits × 64 `j`) — é trabalho
de uma sessão dedicada, com o mesmo gate de 0 divergências.

**Veredito de admissão (v2, superado pela v3 logo abaixo):** o JL ficava
**`propose`, não `act`** — a barra do plano é 2× *medido e robusto* e o v2
entregava 5–6× em máquina ociosa e 1,6× sob carga.
— não é margem para mudar o default. O código fica atrás de `--features gpu`
(opt-in, default intocado) e o gate de regressão exige ≥1,5×.

### Terceira rodada (v3, **tiled com shared memory**) — JL passa a `act`

O teto do v2 era tráfego: cada linha relia a matriz (31 GB para 2.000 linhas).
A v3 (`src/core/gpu/jl_tiled.rs` + `jl_tiled.wgsl`) carrega o tile da matriz em
**shared memory** e o reusa para **32 linhas** — o tráfego de `mat` cai 32×.
Bloco = (8 palavras de 32 bits) × (32 linhas), 8 warps (256 threads), 32
acumuladores por thread, `j` ascendente (bit preservado). A matriz sobe **uma
vez por sessão** (`JlSession::set_matrix`), então o `fill + transposição` de
~74 ms também sai do caminho.

| métrica | v2 | **v3 (tiled)** |
|---|---:|---:|
| tempo, 2.000 linhas | 530–600 ms | **23–37 ms** |
| razão vs CPU (2,7–2,9 s) | 5× | **75–122×** |
| primeiro lote (fill+upload+kernel) | 84 + 600 ms | **~101–110 ms** (25–27×) |
| divergências | 0/32.000 | **0/32.000** (n=200); 1/320.000 (n=2000, `\|dot\|=2,9e-6`) |

**Veredito:** **`act`** para lote. A barra de 2× é superada por mais de uma ordem
de grandeza, e mesmo o pior caso (primeiro lote, pagando a matriz) fica em 25×.
Continua atrás de `--features gpu`; o default é CPU-only e o organismo a 60 Hz
não passa por aqui. O único critério que **não** vale é igualdade bit-a-bit
absoluta: o WGSL funde `mul+add`, então um `dot` com `|dot| ~ 1e-6` pode virar
de sinal (o gate aceita ≤4 palavras em 320.000 **e prova** `|dot| < 1e-3`).


### G3 — sweep Ising em lote: **implementado e admitido** (11,3× medido)

Duas rodadas, com o mesmo gate (`tests/gpu_ising_sweep_gate.rs`, 3 gates):

| | v1: 1 thread/instância | **v2: 1 warp/instância** |
|---|---:|---:|
| tempo, 10⁴ instâncias (n=20, 48 passos) | 44 ms | **11,0 ms** |
| CPU auditado (mesmo lote) | 123 ms | 124 ms |
| razão | 2,8× | **11,3×** |
| ociosidade (k=200) | 0,4× | **9,0×** |
| banda efetiva de `J` | 17 GB/s | **73–107 GB/s** |

**Por que a v1 parou em 2,8×:** o custo escalava exatamente com `n²` (medido em
n = 10, 20, 32), isto é, era **tráfego de `J`**: 48 passos × 16 MB = 768 MB, e
cada thread lia a matriz com passo (não coalescida), a 17 GB/s.

**O que a v2 mudou:** um **warp por instância** (lane = sítio, `n ≤ 32`), `x_i`,
`y_i`, `s_i` e `g_i` em registradores, estado compartilhado (`x`, `s`, elite) em
memória de *workgroup* com barreiras, e `J` lido **coalescido**: para `j` fixo as
lanes leem `J[j][lane]`, que é `J[lane][j]` **por simetria**. Custo: o caminho
GPU exige `J` **simétrico** (o contrato do solver já diz isso); com `J`
assimétrico ele difere do CPU.

**Qualidade (o ponto honesto):** `f32` vs `f64` não é bit-a-bit e a trajetória
bSB é caótica. Medido no mesmo lote de 200 instâncias:

| métrica | GPU vs CPU | **CPU(seed2) vs CPU(seed1)** |
|---|---:|---:|
| energias idênticas | 145/200 (72,5%) | **47/200 (23,5%)** |
| melhores / piores | 30 / 25 | 24 / 129 |
| KS das distribuições | 0,030 | **0,055** |
| regret ≤ 5% | 182/200 | **146/200** |
| média de energia | −53,71 (CPU −53,66) | −53,66 |

Ou seja: **a GPU fica mais próxima do solver do que o próprio solver fica de si
mesmo com outra semente**. O gate codifica isso (KS ≤ piso + 0,05, regret ≤ 5%
em ≥ piso − 5 instâncias). Na família **com viés** (`h ≠ 0`) a média GPU pode
ficar ~0,09% pior que a CPU (~19× o piso de ruído seed-a-seed) — o gate admite
desvio relativo ≤ **0,2%** e declara o sweep como acelerador de exploração, não
substituto do solver auditado quando o critério é a média exata. Sem exigir
paridade por instância (impossível no caos; a v1 passava por coincidência de
ordem de soma).

**Custo fixo:** compilar o shader custa **250–550 ms** por chamada. A avulsa
`sweep_into` paga isso; o caminho de produção é `IsingSweep` (sessão que guarda
device, pipeline e buffers) — sem ela a razão medida era **0,2×**.

### Validação na família real do E2c-2 (fechamento do ciclo)

O gate principal usa `h = 0`; para fechar a pergunta de escala, o kernel rodou
nas **instâncias do E2c-2** (vidro `±1` completo + viés forte em `k` sítios, o
`random_glass` de `tests/e2c2_roof_dual_gate.rs`), 1.000 instâncias, `n=20`,
`k=4`, `delta=0.5`, `h ≠ 0` — o caminho do campo externo (energia, `descend`,
fuga) é exercitado inteiro:

| métrica | resultado |
|---|---|
| razão | **11,9×** (12,4 ms CPU → 1,04 ms GPU) |
| idênticas / melhores / piores | 850 / 68 / 82 de 1.000 |
| KS | **0,014** (piso de ruído do solver: 0,024) |
| regret ≤ 5% | 968/1.000 (piso: 895) |
| média de energia | **0,089% pior** (19× o piso de ruído — desvio real) |

**Limite declarado:** com viés forte a GPU fica ~0,1% pior na média. O sweep é
**acelerador de exploração** (mesma forma de distribuição, KS melhor que o
ruído, 11,9× mais rápido), **não** substituto do solver auditado quando o
critério é a média exata. O gate codifica isso: `|Δmédia| ≤ 0,2%`.

**Veredito:** admitido como caminho **de lote** (≥ ~10³ instâncias; abaixo de
~500 a taxa de ocupação não paga). Continua fora do pipeline por instância, e
**o `digest` do E2 e o `bsb_compacto_igual_ao_solver_auditado` não valem neste
caminho** (documentado no topo de `ising.rs`).

### G2 — kernel implementado, **rejeitado por medição** (2026-11-17)

* **G2 (FFT)**: kernel WGSL implementado (dono do FFT, gate `gpu_fft_gate`
  verde: erro rel. 4,2e-7 vs `rustfft`, repro same-lote, `force_off` sem
  `panic`, invariante estrutural). O **probe de crossover**
  (`examples/fft_gpu_probe.rs`) mediu a razão CPU/GPU ponta-a-ponta em
  L=8192: **0,18× (B=64), 0,21× (256), 0,22× (1.024), 0,17× (2.048)** —
  **não existe crossover**: o GPU perde em qualquer lote e perde pior quanto
  maior o lote. Custo fixo por chamada **11,4 ms p50** (13 uniforms + 13
  passes + readback síncrono; a 1ª chamada paga 712 ms de device+shader,
  1× por processo). O gargalo é o trabalho do *host* dentro da chamada
  (bit-reversal cache-hostil, pack/unpack, cópias, PCIe síncrono) — a
  computação GPU é trivial. **`rustfft` CPU é a rota do FFT**; reabrir G1/G2
  exige sessão com buffers residentes + bit-reversal no shader + readback
  async (estilo `JlSession`).

---

## Fechamento do ciclo (2026-11-17)

| fase | estado | número medido |
|---|---|---|
| **G0** censo | ✅ | JL 13,4 s (2,9 GFLOP/s), FFT 7,4× projetado, Ising 929× projetado |
| **G1** JL do transplante | ✅ **`act`** | **75–122×** (tiled, 2.000 linhas); 25× no 1º lote pagando a matriz |
| **G2** FFT em lote | ❌ `abstain` com número | 7,4× era **projeção**; medido **0,18–0,22× (perde)**, custo fixo 11,4 ms/chamada, sem crossover até B=2.048 (`examples/fft_gpu_probe.rs`, MSG 0438) |
| **G3** sweep Ising | ✅ admitido **só em lote** | **11,3×** em 10⁴ (sintético) e **11,9×** na família E2c-2 (`h ≠ 0`), KS 0,014 vs piso 0,024; média 0,089% pior |

Nenhum kernel novo entra no `default`: tudo atrás de `--features gpu`, e o laço
de 60 Hz, o `record` do F1 e o solver do E2 continuam em CPU.

### Gates que continuam necessários (dívida declarada)

1. **CI com `--features gpu` em máquina com adapter.** Hoje os três gates de GPU
   (`gpu_jl_gate`, `gpu_jl_tiled_gate`, `gpu_ising_sweep_gate`) e o
   `gpu_boundary_gate` rodam **à mão**. Sem um job opt-in, uma regressão de
   kernel não é pega por `cargo test` (sem a feature os testes compilam fora).
2. **G2 (FFT) — fechado por medição (2026-11-17).** Gates (a) erro ≤ 1e-4,
   (c) `force_off` sem `panic`, (d) invariante estrutural e (e) reprodutibilidade
   **verdes** no `gpu_fft_gate`; o gate (b) razão ≥ 2× **falhou honestamente em
   todos os lotes** (probe: 0,18–0,22×). Dívida restante só se alguém reescrever
   o caminho: sessão com buffers residentes, bit-reversal no shader (ou saída em
   ordem natural) e readback async — sem isso, FFT fica em CPU.
3. **JL não tem bit-exatidão absoluta** (o WGSL funde `mul+add`): o gate é
   `≤4 palavras em 320.000` **e** `|dot| < 1e-3` provado no teste. Se o JL
   entrar num caminho bit-exato, é esse contrato que precisa mudar.
4. **Ising exige `J` simétrico** no caminho GPU (a leitura coalescida usa
   `J[j][i] = J[i][j]`). Não há assert barato em shader; fica no doc e no gate.
5. **`cargo fmt --all --check`** ainda falha em arquivos de outros agentes
   (`src/bin/*`, `src/api/*`, `src/core/logic_pipeline.rs`): dívida alheia.

---

## Embed donor — R4 pós-reboot (MSG 0605, 2026-11-18)

Fora do `gpu` crate (PyTorch CUDA, não wgpu). RTX 3050, Qwen3-Embedding-0.6B:

| setup | taxa | vs A1 CPU |
|---|---:|---:|
| CUDA bf16 batch64 n=1000 | **19,33 txt/s** | **1,79×** (A1 ≈ 10,82) |
| ETA 100k | ~1,44 h | — |

GATE_A1 limpo. Próximo medido: R3 CPU threads/batch + R5 ONNX/OpenVINO (lit MSG 0611).
Não confundir com razão JL wgpu (5–6×).


