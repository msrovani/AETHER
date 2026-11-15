# AETHER — Relatório de Hardening (2026-11-15)

**Versão:** `0.0.43` (`Cargo.toml`; medições históricas da seção 2 datam de 0.0.35)  
**Escopo:** fases 1–2 (`m1`..`m8`), auditoria independente, correções e medições.

## 1. Gate final

| Verificação | Resultado |
|---|---|
| `cargo test --release --offline` | **95 testes** (`--list`, 2026-11-15) / **15 suítes** `tests/*.rs` + unitários no lib |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `aether-organism --duration 5s --plain` | R=0,9989; entropia −0,0686; H(s\*)<H(s₀) |
| `aether-daemon --duration 8 --seed 7` | R=0,000/peers=0 sozinho; 468 ciclos; drops=0 |

Windows GNU: usar `scripts/cargo.ps1` (o `rustc` windows-gnu precisa de
`dlltool` + `as` do WinLibs).

## 2. Limitações resolvidas

| Limitação | Correção | Medição |
|---|---|---|
| M5 p0=0,169 rad | `imprint` com **projeção complexa** (pseudo-inversa de Gram, `G⁻¹` transposta, ridge `1e-6·N`, janela FIFO) | p0=**0,000035**, p1=**0,000039**, p2=**0,000085** rad (30 seeds, 12 ocultos); 13 testes |
| M2 L2048=488 µs | benchmark isolado; `fast_tanh`/scratch já corretos; 488 µs era contenção | **35,7 µs** em L2048 (L128 2,54; L512 8,67; L8192 159); meta 300 µs |
| Organismo entropia +0,003 | piso branco físico `sensor_noise=0.35` (SNR ≈11 dB; fase dominante) | redução **−0,051..−0,069**, 100% dos quadros em 5 seeds; R>0,98; H estrito |
| M8 drops/alocações | buffers `spins/bias/pattern`, `field_into`, `imagine_futures_into` | **22 → 0 alocações/ciclo**; 0 drops em 468 ciclos idle; 0–1 drop sob 10 busy-loops |
| M3 `rx_bytes`/peers | dedup de peers + contador zera ao estourar u32, preservando `%40==0` | testes de dedup, overflow (`u32::MAX-20 → 0`) e convergência (R=0,993) |

## 3. Decisões que ficam

### M5 — projeção complexa

```
G_μν = Σ_i conj(ψ_i^μ) ψ_i^ν              (P×P hermitiana + ridge)
C_ij = Σ_μν conj(ψ_i^μ) (G⁻¹)_{νμ} ψ_j^ν
W = |C|/⟨|C|⟩,  δ = arg C
```

A transposta de `G⁻¹` é obrigatória; sem ela `Im(C_ii) ≠ 0` e o crosstalk
retorna. Capacidade de atrator ≈ `N/4` (janela FIFO `min(N/4,64)`); acima disso
os padrões são pontos fixos exatos, mas a bacia encolhe. `recall_into` é
zero-alloc; `prune` é máscara permanente.

### M6 — zero-alloc

`DreamEnsemble::with_capacity` + `imagine_futures_into` + workspace
`x/y/hx` por amostra com `par_chunks_mut`. Contadores de alocação em testes
devem ser **thread-local**; contador global é contaminado pelo harness paralelo.

### M7 — morfogênese

Ganho hebbiano depende da surpresa de reconstrução (`tanh(S)`), não de `F` com
termo metabólico (senão o silêncio cresce). Homeostase por ρ real (power
iteration, período 2, nunca amplifica); medido ρ≤1,17 entre normalizações e
1,000 pós. `sparsity_ratio` = fração dissolvida; `density()` = fração ativa.

### M8 — daemon

`node_id` único por processo, descarte do próprio pacote, `IP_MULTICAST_LOOP=true`
(necessário para dois processos no mesmo host). `F` absoluto (NMSE) alimenta o
M7; `S` relativo ao baseline dispara `SONHANDO` (idle `S≈0,01`; 10 busy-loops
`S≈4`). CLI: `--duration` aceita `30`/`30s`; `nan/inf` retorna 2 sem panic.

### Organismo e M2

`sensor_noise=0.35` é o piso branco do sensor: com fase quase-estática
(narrowband) não há energia acima dos 32 modos para o filtro remover; o piso
cria essa banda e a interferência reduz a entropia. O benchmark M2 deve rodar
em máquina ociosa; contenção de CPU inflou 35,7 µs para 488 µs numa medição.

## 4. Limites inerentes (não são bugs)

* M5: bacia de atrator ≈`N/4`; para P acima disso, use menos padrões ou uma
  regra de capacidade maior.
* M7: silêncio sustentado leva ρ→0 e poda total (intencional); `J=0 ∧ pe=0` é
  estado absorvente.
* M8: sob build storm (17 `rustc`) o SO ainda preempta e o daemon perde
  quadros; o contador `drops` expõe o evento. O custo do ciclo caiu 2,8×.
* M2: os números variam com frequência de CPU/thermal; o bench criterion é a
  referência, não um valor fixo.

## 5. Fórum e reprodutibilidade

Mensagens finais: `MSG 0049` (gate hardening), `MSG 0054`
(limitações resolvidas). Protocolo textual em
`C:\Users\msrov\Desktop\FORUM.txt`; ferramenta `tools/forum_post.ps1`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 test --release --offline
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 clippy --all-targets --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 fmt --all -- --check
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 run --release --offline --bin aether-organism -- --duration 5s --plain
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo.ps1 run --release --offline --bin aether-daemon -- --duration 8 --seed 7
```
