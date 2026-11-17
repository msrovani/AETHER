# Fechamento de ciclo — track **GPU** (G0–G3)

Data: **2026-11-17** · Versão: **0.0.168**  
Tópico: `GPU_G1_G3` · RELATORIO **MSG 0388** (G3 sweep) · **MSG 0410** (JL tiled) · **MSG 042x** (este fechamento)  
Register irmão: `docs/CYCLE_CLOSE_2026-11-17_DOACAO_ONDA0.md` (doação/hub/onda 0 — outro agente)

## O que este track fechou

| Fase | Entrega | Número **medido** (RTX 3050 6 GB) |
|---|---|---|
| **G0** censo | `examples/gpu_census.rs` | JL 13,4 s (2,9 GFLOP/s) · FFT 7,4× projetado · Ising 929× projetado |
| **G1** JL do transplante | v1 átomos → v2 coalescido → **v3 tiled** (`jl_tiled.rs` + `JlSession`) | **75–122×** vs CPU em 2.000 linhas (23–37 ms vs 2,7–2,9 s); **25–27×** já no 1º lote pagando `fill` (74 ms) + upload |
| **G2** FFT em lote | **não implementado** (decisão) | 7,4× segue **projetado**; sem consumidor de lote (o `WaveInterferenceBlock` roda no laço de 60 Hz, que é CPU por invariante) |
| **G3** sweep Ising | v1 thread/instância → **v2 warp/instância** (`ising.rs` + `ising.wgsl` + `IsingSweep`) | **11,3–11,4×** em 10⁴ instâncias (11 ms vs 124 ms); **11,9×** na família E2c-2 |
| **JT3** validação na família E2c-2 | `e2c_glass_com_vies_na_gpu` (1.000 instâncias, `h ≠ 0`) | 850/1.000 idênticas · KS **0,014** (piso do solver 0,024) · regret ≤5% 968/1.000 (piso 895) · **média 0,089% pior** |

## O que cada rodada ensinou (e por que a segunda ganhou)

| kernel | v1 → v2 | diagnóstico | cura |
|---|---|---|---|
| JL | 3× → 5–6× | 2.000 linhas × 15,7 MB = **31 GB** relidos (cada linha relê a matriz) | tile da matriz em **shared memory**, reuso por 32 linhas |
| Ising | 2,8× → 11,3× | custo escalava **exatamente com n²** → tráfego de `J` (768 MB) a 17 GB/s | **warp por instância** + `J` coalescido pela simetria (`J[j][lane] = J[lane][j]`) |

Os dois foram **medidos**, não inferidos: a escala em `n` (Ising) e por linha (JL)
mostrou que o gargalo era banda, não FLOP. E em ambos o custo fixo de compilar o
shader (**250–550 ms** por chamada) tornava a API avulsa inútil (0,2× no Ising):
o caminho passou a ser sessão (`IsingSweep`, `JlSession`).

## Correção / determinismo (contrato declarado)

* **JL**: bit idêntico ao CPU, exceto `dot` com `|dot| < 1e-3` — o WGSL funde
  `mul+add` e um quase-zero pode virar de sinal: **0/32.000** em n=200 e n=37;
  **1/320.000** em n=2.000 com `|dot| = 2,9e-6` (o gate recalcula o dot em
  `f64` e **prova** `|dot| < 1e-3`; qualquer outra divergência falha).
* **Ising**: `f32` vs `f64` num sistema caótico. O gate **não** exige paridade
  por instância: compara com o **piso de ruído do próprio solver** (CPU com
  outra semente: 47/200 idênticas, KS 0,055) contra a GPU (145/200, KS 0,030).
  A GPU está mais perto do solver do que o solver está de si mesmo.
* **Invariantes preservados**: 60 Hz, `record` do F1, T0 e o solver do E2 não
  passam por `core::gpu`; `gpu` continua fora do `default`.

## Gates verdes neste ciclo

| Gate | Critério | Resultado |
|---|---|---|
| `tests/gpu_jl_gate` | v1/v2 bit-exato + ≥1,5× + fallback | 4/4 PASS |
| `tests/gpu_jl_tiled_gate` | bit-exato (+ prova de `|dot|`) + ≥10× + sessão + fallback/shape | 3/3 PASS (**75–122×**) |
| `tests/gpu_ising_sweep_gate` | KS ≤ piso+0,05 · regret ≤ piso−5 · média ≤ piso+0,2% · ≥2× · fallback | 4/4 PASS (+2 diagnósticos `#[ignore]`) |
| `tests/gpu_boundary_gate` | fronteira do crate sem a feature | 3/3 PASS |
| `cargo test --release --tests` (sem feature) | suíte completa | PASS (lib 165 + todas) |
| `cargo test --release --features gpu --tests` | suíte completa com GPU | PASS |

## Gates / frentes **ainda necessários** (próximo ciclo)

| ID | Gate | Por que ficou aberto |
|---|---|---|
| **CI gpu** | job opt-in `--features gpu` em máquina **com adapter** | os 4 gates de GPU rodam à mão; sem a feature eles compilam fora (`#![cfg(feature = "gpu")]`) e o CI padrão não vê regressão de kernel |
| **FFT-G2** | (a) erro relativo ≤ 1e-4 vs `rustfft` em `L=8192, B=64`; (b) razão ≥ 2× **medida**; (c) `force_off` sem `panic`; (d) invariante estrutural (organismo 60 Hz não referencia `core::gpu`); (e) mesma saída para o mesmo lote | sem consumidor de lote hoje → `abstain` honesto, não `fail` |
| **JL-bit-exato** | se o JL entrar em caminho bit-exato, o contrato `|dot| < 1e-3` precisa mudar (FMA do WGSL) | hoje o JL é `act` só em lote, fora dos gates bit-exatos do E2 |
| **Ising-simetria** | `J` simétrico no caminho GPU (a leitura coalescida usa `J[j][i] = J[i][j]`) | não há assert barato em shader; fica no doc + no gate |
| **Ising-média** | família com viés: GPU 0,089% pior na média (19× o piso de ruído) | é acelerador de exploração, não substituto do solver quando o critério é a média exata |

## Receita de reprodução

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# gates de GPU (precisam de adapter)
cargo test --release --features gpu --test gpu_jl_tiled_gate -- --nocapture
cargo test --release --features gpu --test gpu_ising_sweep_gate -- --nocapture
cargo test --release --features gpu --test gpu_jl_gate
cargo test --release --features gpu --test gpu_boundary_gate

# diagnóstico (custos fixos e escala em n)
cargo test --release --features gpu --test gpu_ising_sweep_gate bench_bruto -- --ignored --nocapture
cargo test --release --features gpu --test gpu_ising_sweep_gate bench_escala_n -- --ignored --nocapture
```

## Aprendizados para a memória do ciclo

1. **Meça a dimensão antes de otimizar**: escalar o custo em `n` (ou em linhas)
   separa “falta FLOP” de “falta banda”. Nos dois kernels o veredito foi banda.
2. **Shared memory com reuso por bloco** é o multiplicador: 32× no JL, e no
   Ising o ganho veio de trocar arrays por thread (memória local) por registros
   + *workgroup memory*.
3. **Compilar shader custa 250–550 ms**: sem sessão persistente o ganho
   desaparece (0,2× no Ising, 1,6× no JL com `fill` por chamada).
4. **Solver caótico não tem paridade por instância**: o gate correto é o piso
   de ruído do próprio solver (outra semente), mais a prova de que a energia
   reportada é `H(estado devolvido)`.
5. **`ACT` exige número robusto**: JL só virou `act` quando o pior caso (1º
   lote, pagando a matriz) ficou em 25× — não quando o melhor caso deu 122×.
