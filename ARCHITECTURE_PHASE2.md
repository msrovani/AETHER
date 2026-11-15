# PROJETO AETHER — FASE 2: Memória de Fase, Imaginação Termodinâmica e Morfogênese

> Especificação da segunda fase do motor AETHER. Expande `aether_engine` mantendo
> os axiomas da Fase 1 (Módulos 1–4). Implementação no crate Rust `aether_engine`,
> com os módulos atrás das features `m5`, `m6`, `m7` e `m8`.

## 1. Princípios Invioláveis da Fase 2

Estes módulos expandem o motor mantendo os axiomas originais:

- **ZERO Tokens / Strings / Embeddings:** toda memória, sonho contrafactual e
  plasticidade estrutural opera exclusivamente sobre fases contínuas
  $\phi \in [0, 2\pi)$, espectros complexos e spins de Ising $s_i \in \{-1,+1\}$.
  `String` só aparece em mensagens de erro e rótulos de exibição — nunca como
  entrada ou estado de sinal.
- **ZERO Backpropagation:** o aprendizado de padrões e o rearranjo topológico
  ocorrem localmente por regras hebbianas de diferença de fase e minimização de
  Energia Livre Variacional ($\mathcal{F}$).
- **ZERO Alocação nos Loops Críticos:** todos os tensores, matrizes complexas e
  buffers de amostragem são pré-alocados na inicialização das estruturas.
  `imprint`/`recall`, `imagine_futures` e `evolve_topology` não alocam após o
  *warmup* (o mesmo padrão de `relax_into`/`forward_into` da Fase 1).
- **Compatibilidade com a Fase 1:** o Módulo 6 reutiliza o layout plano `N×N`
  do `J` do Módulo 1; o Módulo 7 consome o `WaveInterferenceBlock` do Módulo 2;
  o Módulo 8 fala o protocolo binário de 40 bytes do Módulo 3 e usa o painel
  ANSI do Módulo 4.

## 2. Novos Arquivos

```text
aether_engine/
├── ARCHITECTURE_PHASE2.md
├── src/
│   ├── lib.rs                       # declara os módulos 5, 6 e 7 (features m5/m6/m7)
│   ├── phase_memory.rs              # Módulo 5: Memória Associativa Oscilatória (ONN)
│   ├── thermodynamic_dreamer.rs     # Módulo 6: GSB na borda do caos + Langevin
│   ├── morphogenesis.rs             # Módulo 7: plasticidade topológica por energia livre
│   └── bin/
│       └── aether_daemon.rs         # Módulo 8: transdutor físico real + enxame LAN
└── tests/
    └── phase2_invariants.rs
```

## 3. Especificação Matemática e de Implementação

### Módulo 5: `src/phase_memory.rs` — Memória Associativa por Atratores de Fase

Rede Neural Oscilatória (ONN) com **acoplamento complexo**
$C_{ij} = W_{ij} e^{i\delta_{i,j}}$, onde o ângulo de deslocamento
$\delta_{i,j} \in [0,2\pi)$ codifica relações de fase arbitrárias (padrões
contínuos ricos, não apenas binários $0/\pi$):

$$\frac{d\phi_i}{dt} = \omega_0 + \frac{1}{N} \sum_{j} W_{ij}
\sin(\phi_j - \phi_i - \delta_{i,j}) + \alpha_{\text{clamp}}\, m_i
\sin(\phi_i^{\text{cue}} - \phi_i)$$

**Aprendizado hebbiano local (gravação sem backprop):**

$$\Delta W_{ij} = \eta_W \big(\cos(\psi_j - \psi_i - \delta_{i,j}) - \gamma_W W_{ij}\big)$$

$$\Delta \delta_{i,j} = \eta_\delta\, W_{ij} \sin(\psi_j - \psi_i - \delta_{i,j})$$

**Recuperação associativa (pattern completion):** com $m_i = 1$ nos nós
conhecidos e $m_i = 0$ nos mascarados/corrompidos, o relaxamento reconstroi as
fases faltantes no mínimo da energia de fase

$$E_{\text{phase}}(\boldsymbol{\phi}) = -\frac{1}{2N} \sum_{i,j} W_{ij}
\cos(\phi_j - \phi_i - \delta_{i,j})$$

**Estruturas obrigatórias:**

* `pub struct ComplexPhaseMemory` com `n: usize`, `weights: Vec<f32>`,
  `phase_shifts: Vec<f32>`, `phases: Vec<f32>`.
* `pub fn imprint(&mut self, pattern: &[f32], cycles: usize)` — grava um
  atrator contínuo (padrão de fases em $[0,2\pi)$).
* `pub fn recall(&mut self, partial_cue: &[f32], mask: &[bool], steps: usize,
  dt: f32) -> (Vec<f32>, f32)` — retorna o padrão completo reconstruído e a
  energia final $E_{\text{phase}}$.

**Invariantes (testes):** com 3 padrões ortogonais imprints em $N = 32$ e 40%
dos osciladores mascarados, o erro angular médio do `recall` é $< 0.15$ rad.
O `imprint` usa **projeção complexa** (pseudo-inversa):
$G_{\mu\nu}=\sum_i \overline{\psi_i^\mu}\psi_i^\nu$ (ridge $10^{-6}N$),
$C_{ij}=\sum_{\mu\nu}\overline{\psi_i^\mu}(G^{-1})_{\nu\mu}\psi_j^\nu$,
$W=|C|/\langle|C|\rangle$, $\delta=\arg C$. A transposta de $G^{-1}$ é
obrigatória: sem ela $\operatorname{Im}(C_{ii})\neq0$ e o crosstalk volta.
Medido com 30 seeds e 12 ocultos: **p0=0.000035, p1=0.000039, p2=0.000085**
(piores 0.00028/0.00011/0.00063); 0 seeds acima de 0.15. A projeção também
deduplica rotações rígidas e usa janela FIFO `min(N/4,64)`; acima de ~N/4 os
padrões são pontos fixos exatos, mas a bacia encolhe (limite documentado em
`pattern_capacity()`).

### Módulo 6: `src/thermodynamic_dreamer.rs` — Borda do Caos e Imaginação

Evolução do solver determinístico para um **amostrador generativo
termodinâmico**: Bifurcação Simulada Balística Generalizada (GSB) com controle
individual não-linear $a_i(t)$ na borda do caos + flutuações térmicas de
Langevin:

$$dx_i = a_0 y_i\, dt$$

$$dy_i = \Big( -\big[a_0 - a_i(t)\big] x_i + c_0 \big[\sum_j J_{ij} x_j + h_i \big]
- \gamma_{\text{damp}} y_i \Big) dt + \sqrt{2 T dt}\, \eta_i$$

com $\eta_i \sim \mathcal{N}(0,1)$ e

$$a_i(t) = a_{\text{base}}(t) \cdot \left(1.0 - \beta_{\text{chaos}}
\tanh\left|\sum_j J_{ij} x_j\right|\right)$$

**Dois regimes cognitivos na mesma equação:**

1. **Ação ($T \to 0$):** colapso rápido e determinístico para decisão motora.
2. **Imaginação ($T > 0$):** $K$ trajetórias paralelas via `rayon`, amostrando
   $\{\mathbf{s}^{(k)}\} \sim P(\mathbf{s}) \propto e^{-H(\mathbf{s})/T}$.

**Estruturas obrigatórias:**

* `pub struct ThermodynamicDreamer` reutilizando o layout plano `N×N` de `J`.
* `pub fn imagine_futures(&mut self, j: &[f32], h: &[f32], temperature: f32,
  num_samples: usize, steps: usize, dt: f32) -> DreamEnsemble` retornando as
  amostras $\{\mathbf{s}^{(k)}\}$, a energia média $\langle H \rangle$ e a
  **Incerteza Epistêmica** $\mathcal{U} \in [0,1]$ (variância média dos spins
  entre os futuros amostrados).

**Invariantes (testes):** sobre a mesma `J` frustrada ($N=32$, 32 amostras),
$\mathcal{U}(T_{\text{alto}}) > \mathcal{U}(T_{\text{baixo}})$ e todas as
amostras têm energia menor que a de estados aleatórios.

### Módulo 7: `src/morphogenesis.rs` — Plasticidade Topológica por Energia Livre

Auto-reconfiguração estrutural de $J_{ij}$ em tempo real, guiada pela Energia
Livre Variacional (Surpresa):

$$\mathcal{F}(t) = \frac{1}{L}\sum_{x} \big(u(x,t) - \hat{v}(x,t)\big)^2
+ \lambda_{\text{metabolic}} \sum_{i,j} |J_{ij}|$$

**Dinâmica morfogenética (crescimento hebbiano modulado por surpresa + poda
metabólica):**

$$\frac{dJ_{ij}}{dt} = \kappa_{\text{grow}} \tanh(\mathcal{F}(t))
\big(s_i s_j - J_{ij}\big) - \kappa_{\text{decay}} J_{ij}$$

* **Cristalização e dissolução:** $|J_{ij}| < \theta_{\text{dissolve}}$
  (padrão 0.005) ⇒ $J_{ij} = 0$ (poda sináptica física). Simetria estrita
  $J_{ij} = J_{ji}$ e diagonal nula garantidas a cada passo.
* **Homeostase espectral:** normalização periódica das linhas para manter
  $\rho(J) \approx 1.0$ (regime crítico).

**Estruturas obrigatórias:**

* `pub struct MorphogeneticField` — evolução contínua de `J` +
  `sparsity_ratio`.
* `pub fn evolve_topology(&mut self, prediction_error: f32, active_spins:
  &[f32], dt: f32) -> f32` — retorna a Energia Livre atualizada $\mathcal{F}$.

**Invariantes (testes):** 500 ciclos alternando surpresa alta ($\mathcal{F} =
2.0$) e silêncio ($\mathcal{F} = 0$): (a) `|J_ij − J_ji| < 1e-6`, (b) sinapses
ociosas podadas (`sparsity_ratio` sobe no silêncio), (c) nunca `NaN`/`Inf`.

**Semântica após o hardening:** `sparsity_ratio()` é a **fração dissolvida**
(0 = grafo completo, 1 = vazio); `density()` é a fração ativa. A homeostase usa
iteração de potência com período 2; $\rho$ medido ≤ 1.17 entre normalizações e
1.000 após. O ganho hebbiano usa a surpresa de reconstrução (o termo metabólico
penaliza a densidade, não alimenta o crescimento). O `tests/phase2_m7_hardening.rs`
cobre silêncio (podação), surpresa (regeneração), 2000 ciclos e bordas.

### Módulo 8: `src/bin/aether_daemon.rs` — Organismo Encarnado em LAN Multicast

Daemon autônomo que conecta o núcleo ao mundo físico real da máquina e à rede
local:

1. **Transdutor Sensorial de Hardware (zero strings):** lê métricas físicas
   reais do sistema (jitter de nanossegundos entre acordares de thread, carga
   de CPU por núcleo, taxa de variação de pacotes de rede via API nativa do SO)
   e converte diretamente num campo de ondas contínuo $u(x,t) \in \mathbb{R}^{64}$
   por modulação de frequência/fase.
2. **Descoberta automática P2P via UDP Multicast (`239.255.42.99:7777`):**
   emite e escuta os frames binários de 40 bytes (`PhasePacket` do Módulo 3) em
   multicast local. Qualquer `aether_daemon` na mesma rede acopla suas fases
   automaticamente, sem configuração de IP.
3. **Ciclo cognitivo unificado (60 Hz):**
   * captura a onda física do hardware + fases dos vizinhos da LAN;
   * filtra/prediz via `WaveInterferenceBlock` e calcula a Surpresa
     $\mathcal{F}(t)$;
   * se $\mathcal{F} > \mathcal{F}_{\text{alert}}$: dispara
     `ThermodynamicDreamer::imagine_futures` ($T = 0.45$) para avaliar
     contrafactuais; caso contrário colapsa ação ($T = 0.01$) e grava o padrão
     estável na `ComplexPhaseMemory`;
   * remodela a topologia via `MorphogeneticField::evolve_topology`;
   * exibe telemetria viva: Coerência LAN $R(t)$, Surpresa $\mathcal{F}(t)$,
     Energia de Memória $E_{\text{phase}}$, Incerteza Epistêmica
     $\mathcal{U}$, Densidade Topológica (%) e Latência do Ciclo (µs).

**Medições após o hardening do M8:** uma instância sozinha reporta
`R(LAN)=0.000, peers=0`; duas instâncias no mesmo host, `R≥0.95, peers=1`
(loop multicast ligado + descarte do próprio pacote por `node_id` único). O CLI
aceita `--duration 30`/`30s`, `--hz`, `--seed`, `--alert`, `--window`, `--dim`,
`--samples`, `--steps`; valores não finitos retornam erro 2 sem panic. O
transdutor mede jitter antes de avançar o prazo e usa a mesma unidade de
trabalho na calibração; `S` (surpresa relativa ao baseline) dispara `SONHANDO`
sob carga (medido `S=2.9`), enquanto `F` absoluto alimenta a morfogênese. O
daemon ainda **não** consome `MorphogeneticField::coupling()` no sonhador
(usa `j_field` estático); ver limitações no relatório do fórum.

## 4. Critérios de Aceitação (`tests/phase2_invariants.rs`)

`cargo test --release` valida obrigatoriamente:

1. **`test_complex_phase_memory_recall`:** 3 padrões ortogonais imprints em
   `ComplexPhaseMemory` ($N = 32$); com 40% dos osciladores mascarados, o erro
   angular médio do `recall()` é $< 0.15$ rad para p0/p1/p2 (medido:
   $<10^{-3}$ com a projeção complexa; `tests/phase2_m5_hardening.rs` fixa o
   envelope de regressão).
2. **`test_thermodynamic_dreamer_uncertainty`:** $\mathcal{U}(T=1.5) >
   \mathcal{U}(T=0.001)$ sobre a mesma `J` frustrada ($N = 32$, 32 amostras) e
   todas as amostras com energia menor que estados aleatórios.
3. **`test_morphogenetic_homeostasis_and_pruning`:** 500 ciclos alternando
   $\mathcal{F} = 2.0$ e $\mathcal{F} = 0.0$: simetria
   $|J_{ij} - J_{ji}| < 10^{-6}$, poda no silêncio (`sparsity_ratio` sobe) e
   ausência total de `NaN`/`Inf`.

## 5. Comandos

```bash
cargo test --release                                  # Fase 1 + Fase 2
cargo run --release --bin aether-daemon -- --duration 30s
# em duas máquinas (ou dois terminais) da mesma rede:
# abra um compilador / estresse a CPU em uma e observe Surpresa e Incerteza
# subirem por ressonância de fase na outra — sem um único pacote JSON.
```

## 6. Medições de hardening (2026-11-15, v0.0.33)

* **M5 (projeção complexa):** p0 = 0,000035 / p1 = 0,000039 / p2 = 0,000085 rad
  (30 seeds, 12 ocultos); 13 testes em `tests/phase2_m5_hardening.rs`.
* **M2 (bench isolado, `benches/wave.rs`):** `forward_into` L128 = 2,54 µs,
  L512 = 8,67 µs, **L2048 = 35,7 µs**, L8192 = 159 µs (meta 300 µs; 8,4× de
  folga). Os 488 µs de uma medição concorrente eram contenção de CPU.
* **Organismo (M4):** `sensor_noise = 0.35` (piso branco do sensor; SNR ≈ 11 dB,
  fase ainda dominante) faz a interferência reduzir a entropia: −0,051 a
  −0,069 em 3 execuções, 100% dos quadros negativos em 5 seeds; R > 0,98 e
  H(s*) < H(s0) estritos no quadro final.
* **M8 (daemon):** 22 → 0 alocações por ciclo (buffers de spins/bias/pattern,
  `field_into`, `imagine_futures_into`); 0 drops em 468 ciclos/8 s idle;
  0–1 drop sob 10 busy-loops. Build storms (17 `rustc`) ainda causam drops por
  preempção do SO — o contador expõe, não é custo do ciclo.
* **M3:** peers deduplicados e `rx_bytes` zera ao estourar `u32` (preserva
  `%40==0`); testes de dedup e overflow adicionados.
* Gate final: **79 testes / 17 suítes**, `cargo test --release` 2×, clippy
  `-D warnings` e `fmt --check` verdes.
