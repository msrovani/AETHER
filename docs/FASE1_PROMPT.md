# FASE 1 — Prompt técnico executável (`E2c-2` · `E1b-2` · `F1`)

**Para:** agente implementador (1 claim por arquivo, ver `FORUM_PROTOCOL.md` §4).
**Objetivo:** fechar as três pré-condições de base antes dos módulos cognitivos
(`E1`/`E2`/`E3`), sem violar os invariantes do núcleo.

**Invariantes invioláveis (o build quebra se violar):**

| Invariante | Como é verificado |
|---|---|
| `0` tokens / `0` `String` no sinal | revisão de tipos: `KnowledgeEvent`, `KnowledgeSink`, `KnowledgeSource` só transportam POD/`[f32]`; nenhum `String`/`&str` atravessa o sinal (a borda texto↔onda é `TransducerIo`, `tests/transducer_invariants.rs`) |
| `0` backprop | nenhum `grad`/optimizer no caminho; `E8` congela o campo |
| `0 B` alocados no *hot-path* | `tests/alloc_invariants.rs` (allocador contador por thread) estendido em `tests/f1_knowledge_bridge_gate.rs` |
| Residência L2 (1,25 MB) | `docs/HW_TARGET.md`; a ponte `F1` adiciona **≤ 45 KB** (ring de 64 eventos) |
| Orçamento de ciclo | margem medida `81,3 µs`/ciclo (`0,49%` de 60 Hz) — `E6` |

**Não-objetivos desta fase:** implementar `E1`/`E2`/`E3`; escrever no SGDB;
alterar `src/core/spsc_ring.rs` (congelado, tem dono); adicionar dependências
(`Cargo.toml` só ganha a feature `knowledge`); tunar limiar depois de ver o
resultado (ver §6, regra anti-*overfit*).

**Alinhamento com o fórum (MSGs 0106–0131):** a divisão de trabalho é a da
MSG 0122 — `E2c-2`, `E1b-2` e `F1` ficam com `OPENCODE_A`; `E2`/`E3` com a
MSG 0107 (`logic_pipeline.rs`, `transplant.rs`, `knowledge_bridge.rs`,
`presolve.rs` região `persistency_qumo_into`); `E1` com a MSG 0106; auditoria
read-only com `DEEPSEEK`. Por isso `F1` nasce em
`src/core/knowledge_bridge_f1.rs` (arquivo próprio), e **nada** aqui autoriza
editar arquivo de outro claim.

---

## 0. Estado medido (não re-medir o que já existe)

| Fonte | Arquivo | Números |
|---|---|---|
| `E2c` | `examples/e2c_persistency.rs`, `target/aether_e2c.json` | `n=14`, 200 instâncias, `plant=0,25`: redução **28,57%**, **0** unsound, 190/200 resolvidas (BSB cheio: 188/200) |
| `E2c` braço vidro | `docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL3.md` §11.1 (e `--plant 0.0` para reproduzir) | redução **0%** — a dominância é míope por construção (`src/core/presolve.rs`, doc de `persistency_qumo`). O artefato atual só tem o braço `plant=0,25` |
| `E1b` braço A | `target/aether_e1b.json` | `D=160,K=6`: **64** fatos (recall@1 ≥ 99%); lei `D^0,90` |
| `E1b` braço B | idem + MSG 0129 | erro angular (`n=160`, **hidden = 0,20**, `seed = 0xE1B0001`, `audit_config`, `reset_phases(0)` antes de cada recall): aleatório **0,0344 / 0,1896 / 0,448** em 4/8/16 padrões; harmônico **0,062 / 0,1416 / 0,362**. Os números `0,126 / 0,274` (MSG 0111) eram **carryover** (recall sem reset), fator 3,7× — não usar |
| `E6` | `target/aether_profile_e6.json` | solver 84,4% do ciclo; margem 81,3 µs |

**Dois defeitos encontrados na leitura do código (corrigir nesta fase):**

1. `src/phase_memory.rs:372` documenta `pattern_capacity()` como `min(N−1, 64)`,
   mas a implementação (linha 277) é `(n/4).clamp(2, 64)` ⇒ em `n=160` o valor
   **é 40**, não 159. O docstring está errado. É essa janela de 40 que o plano
   chama de "40 declarados" — e a medição harmônica mostra **~8**, não 40.
2. O braço B do `E1b` **já mediu harmônico**; `E1b-2` não é um módulo novo, é
   uma **curva densa + ressonador + ruído + gate** sobre o que existe.

---

## 1. `E2c-2` — roof dual completo via max-flow

### 1.1 Objetivo

Fixar variáveis de um QUBO `H(s) = −½ sᵀJs − hᵀs` que são constantes em **algum
ótimo**, usando o *roof dual* (max-flow sobre a posiform), e medir se isso
alcança o que a **dominância** (`persistency_qumo`) não vê — em especial no braço
`plant = 0`, onde a dominância dá **0%**.

### 1.2 Matemática — conversão exata para posiform (implemente assim)

Com `x ∈ {0,1}ⁿ`, `s_i = 2x_i − 1`, `J` simétrica e de diagonal zero:

```text
H(x) = a0 + Σ_i d_i·x_i + Σ_{i<j} Q_ij·x_i·x_j

a0  = Σ_i h_i − Σ_{i<j} J_ij
d_i = 2·(Σ_{j≠i} J_ij) − 2·h_i
Q_ij = −4·J_ij
```

**Teste de âncora obrigatório** (é o que pega erro de álgebra antes de qualquer
teoria): para `n ≤ 8`, 256 atribuições `x` aleatórias, comparar
`a0 + Σ d_i x_i + Σ Q_ij x_i x_j` com `certificate::energy(j, h, n, s)` —
igualdade em `1e-12`. Sem esse teste verde, não avance.

> As três fórmulas acima já foram verificadas numericamente antes de escrever
> este documento: `n ∈ {1,2,5,8}`, 400 instâncias × 20 atribuições, erro máximo
> `7,1e-15`. Se o seu teste de âncora falhar, o erro está na sua transcrição.

**Posiform** (coeficientes não negativos, `T_k` = produto de ≤ 2 literais).
Processe **primeiro todos os quadráticos** (`i<j`, acumulando em `d`) e **só
depois** os lineares — a ordem importa porque `d_i` muda de sinal no meio:

```text
Q_ij ≥ 0 :  termo (a = Q_ij,  lits = (x_i, x_j))
Q_ij < 0 :  d_i += Q_ij ;  termo (a = |Q_ij|, lits = (x_i, x̄_j))
            [identidade: Q·x_i·x_j = Q·x_i + |Q|·x_i·x̄_j]
d_i ≥ 0  :  termo (a = d_i,  lit = x_i)
d_i < 0  :  a0 += d_i ;  termo (a = |d_i|, lit = x̄_i)
```

Resultado: `H(x) = a0 + Σ_k a_k·T_k(x)`, `a_k ≥ 0`. Reporte `a0` e `Σ a_k` no
artefato — são o "tamanho" do problema dual.

### 1.3 Rede, max-flow e regra de persistência

Construa a rede de implicação padrão do roof dual (Boros–Hammer; implementação
de referência: `dwave-preprocessing::fix_variables_roof_duality`), com um nó por
**literal** (`x_i` e `x̄_i`), mais `s` e `t`, e uma aresta por termo da posiform
com capacidade `a_k`; resolva com **Dinic** determinístico (BFS com ordem fixa
de vértices e arestas — nada de `HashMap` na iteração).

**A orientação exata das arestas é o ponto onde implementações divergem.**
Não confie na memória: ancore em dois testes, nesta ordem:

* **T-exato (submodular).** Para `J_ij ≥ 0` (ferromagnético) todos os `Q_ij ≤ 0`
  ⇒ a função é submodular ⇒ o min-cut é **exato**:
  `a0 + maxflow == min_s H` por força bruta (`n = 10`, 20 instâncias, `1e-9`).
  Se falhar, a construção da rede está errada — não siga.
* **T-persistência.** Com a rede validada, a regra é: seja `S` o lado da fonte
  no min-cut (`s ∈ S`, `t ∉ S`);
  `x_i ∈ S ∧ x̄_i ∉ S` ⇒ `x_i = 1` é persistente;
  `x̄_i ∈ S ∧ x_i ∉ S` ⇒ `x_i = 0` é persistente;
  ambos do mesmo lado ⇒ livre (conte em `unresolved`).
  Toda fixação persistente **preserva o valor ótimo** — se alguma alterar o
  ótimo no gate G1, é bug de construção, **reverta**, não relaxe o gate.

#### 1.3.1 O que o protótipo já mediu (não re-descobrir)

Rede canônica do `implication_network.hpp` (dwave-preprocessing), **simétrica
sob complementação**:

```text
termo a·u·v  →  u → comp(v)   E   v → comp(u)
termo a·u    →  S → comp(u)   E   u → T          [comp(S) = T]
bound        =  a0 + fluxo_máximo / 2            ← o /2 é obrigatório
```

Medições (protótipo fora do repo, 600 instâncias contra força bruta):

| Medida | Resultado |
|---|---|
| âncora posiform × energia | erro máx **8,9e-15** |
| exatidão submodular (`J ≥ 0`), `n=10`, 30 inst. | `|a0 + fluxo/2 − ótimo|` máx **7,1e-15** |
| `a0 + fluxo` (sem o `/2`) | erro **51,8** — “parece bound”, está errado |
| persistência BHT completo, `n=14`, `plant ∈ {0; 0,25; 0,45}` | **0,0%** de redução, 0 conflitos, bound sempre válido |

**Duas armadilhas medidas:**

1. rede sem aresta para `T` (só `s→ū`) ⇒ fluxo 0 e bound trivial (`a0`);
2. alcançabilidade **sem** simetrizar o residual ⇒ **46/200 fixações unsound**
   com redução aparente de **14%**. Redução alta não é sinal de acerto.

**Consequência para o gate de alcance:** “roof > dominância no `plant = 0`” pode
ser **falsificado** em vidro de spin denso (`J = ±1`). Nesse caso o resultado é
“o roof dual não vê o vidro” — reporte; não relaxe o gate.

> Em aberto: a implementação `RoofDualWorkspace` de MSG 0110 usa a rede
> **dobrada do QPBO** (outra rota, com `roof_weights`); o protótipo acima não a
> exercita porque a `lib` estava vermelha (`E0583`) na hora da medição.

### 1.4 API (sem alocação no caminho de aplicação)

```rust
// src/core/roof_dual.rs  (novo; gated por `m1`)
pub struct RoofDual {
    pub fixed: Vec<Option<i8>>,   // Some(v) = existe ótimo com s_i = v
    pub residual: Vec<usize>,
    pub bound: f64,               // cota inferior rigorosa (a0 + maxflow)
    pub flow: f64,
    pub unresolved: usize,        // literais empatados no mesmo lado
    pub terms: usize,             // termos da posiform
}

pub struct RoofScratch { /* nós 2n+2, arestas ≤ 2·termos, pré-alocados */ }

pub fn roof_dual(n: usize, j: &[f64], h: &[f64]) -> RoofDual;          // conveniência
pub fn roof_dual_into(n: usize, j: &[f64], h: &[f64],
                      scratch: &mut RoofScratch, out: &mut RoofDual) -> Result<()>; // 0 alloc
pub fn posiform_terms(n: usize, j: &[f64], h: &[f64],
                      a0: &mut f64, terms: &mut Vec<PosiformTerm>) -> Result<()>; // só p/ teste/relatório
```

`RoofScratch::with_capacity(n)` aloca uma vez; `roof_dual_into` **não** aloca
após o warmup (verificado com o allocador contador).

### 1.5 Experimento (`examples/e2c2_roof_dual.rs`)

Mesmo harness do `E2c` (reutilize `make_instance`/força bruta, copiando, não
importando) e três braços:

| braço | instâncias | `n` | `plant` | pergunta |
|---|---|---|---|---|
| A | 200 | 14 | 0,25 | roof ≥ dominância? |
| B | 200 | 14 | 0,0 | roof > 0% onde a dominância é 0%? |
| C | 50 | 18 | 0,25 e 0,0 | soundness em `n` maior (força bruta `2^18`) |

Registre por instância: `reduction`, `bound`, `opt` (bruto), `gap = opt − bound`,
`fixed_roof`, `fixed_dom`, `containment = |dom ∩ roof| / |dom|`, `ms`.

Saída: tabela no stdout + `target/aether_e2c2.json` (mesmo estilo de
`aether_e2c.json`: objetos com `mean`, `min`, `max`, `unsound`, `seeds`).

### 1.6 Gates

| # | Gate | Falha ⇒ |
|---|---|---|
| **G0** | `posiform` bate `energy` em 1e-12 (256×, `n ≤ 8`) | bug de álgebra: corrija |
| **G1** | **0** ótimos alterados em 600 instâncias (`n=14`, dois braços) e 0/50 em `n=18` | **reverta** a persistência; não negocie |
| **G2** | `reduction(plant=0) > 0%` (dominância: 0%) | registre como **falsificação** e escale no fórum — o argumento do `E2c-2` cai |
| **G3** | `reduction(plant=0,25) ≥ reduction_dom + 5 pp` | reporte o delta real; `< 5 pp` significa que o roof dual não paga o custo e **isso é o achado** — vá para o fórum |
| **G4** | `bound ≤ opt` em 100% das instâncias; `gap` médio reportado | bug de bound ⇒ corrija antes de G1 |
| **G5** | determinismo: mesmo `seed` ⇒ fixações idênticas (bit a bit) | ordenação não determinística no Dinic |
| **G6** | custo: p99 `< 2 ms` em `n=160` e `< 50 ms` em `n=640` (release) | otimize ou registre como inviável para T1 |

`G2` e `G3` são **medições**, não promessas: se derem 0%, o resultado é "o roof
dual não vê o vidro de spin" — e isso muda a Fase 3 (`E2` deve usar presolve de
CNF como etapa 1, não confiar no dual).

### 1.7 Entregáveis

`src/core/roof_dual.rs` · `src/core/mod.rs` (1 linha) ·
`examples/e2c2_roof_dual.rs` · `tests/e2c2_roof_dual.rs` (G0, G1 em `n=14`,
G4, G5) · `target/aether_e2c2.json` · post `RELATORIO` no fórum com a tabela.

---

## 2. `E1b-2` — capacidade harmônica real do M5

### 2.1 Objetivo

Transformar o braço B do `E1b` (que já roda aleatório × harmônico) em uma
**curva de capacidade com intervalo de confiança**, testar o ressonador
(ACF/IMF) e produzir **o número que a Fase 2 vai usar**: quantos padrões cabem
por shard em `n=160` com margem de segurança.

### 2.2 O que existe e o que falta

Existe: `ComplexPhaseMemory::{imprint, reset_phases, recall, recall_into,
cleanup, pattern_similarity, confidence_from_phases}`; `harmonic_pattern(n, order)`
e `mask_exact` em `examples/e1b_capacity.rs`; `wrapped_abs`.

Falta: grade densa, 5 seeds, IC, classe com ressonador, SNR, abstenção, e a
correção do docstring de `pattern_capacity`.

### 2.3 Desenho

* `n = 160`, `hidden = 0,4` (0,2 como controle) — **atenção:** o artefato atual
  **não grava** o `hidden` usado nem a seed; determine o valor que reproduz
  (`0xE1B_0001`, `audit_config()` de `e1b_capacity.rs`) antes de fixar a grade.
* `imprint(cycles = 40)`, mesmo `audit_config()` do `E1b`.
* Grade **densa** (não potências de dois): `k ∈ {4,5,6,7,8,9,10,12,14,16,20,24,32,40,48,64}`.
* 5 seeds (`0xE1B2_0001..0005`); reporte média, mediana, mín/máx e o **pior caso**.
* Três classes:
  1. `Random` — `rng.gen::<f32>() * TAU`;
  2. `Harmonic(order)` — o `harmonic_pattern` existente (`sin(TAU·(order+1)·x)·2`
     deslocado) — **reproduza o artefato antes de mexer**;
  3. `Resonator { sigma_init, rounds }` — protótipo ACF/IMF: fases iniciais
     recebem jitter fixo `σ_init` (semeado) e o padrão passa por `rounds`
     ciclos de `imprint` + `cleanup` para ortogonalizar no toro de fase.
     Varra `σ_init ∈ {0; 0,05; 0,1; 0,2}` × `rounds ∈ {1,2,4,8}`.
     **Isto é falsificável por desenho:** ou expande a capacidade a `0,15 rad`,
     ou não. Não "ajuste até funcionar".
* Métricas por `(classe, k)`:
  * `err_hidden` (média de `wrapped_abs` nas posições ocultas) — métrica do artefato;
  * `recall@1` via `cleanup` (índice correto);
  * `margin` médio e fração de consultas com `should_abstain() == false`;
  * `snr` (razão entre similaridade do padrão correto e a do segundo);
  * `pattern_capacity()` declarado (para o contraste).
* `capacity_at(0.15)` = maior `k` com **todos** os seeds abaixo de `0,15`
  (robusto) **e** com a mediana abaixo (reporte os dois — se divergirem, o
  fenômeno é uma *cliff*, não um platô, e isso é o achado).

### 2.4 Gates

| # | Gate | Falha ⇒ |
|---|---|---|
| **G1** | calibração: rodar `k ∈ {4,8}` com `hidden ∈ {0,2; 0,4}` e descobrir qual reproduz o artefato (`random@4 = 0,034`, `random@8 = 0,190`, `harmonic@8 = 0,142`, tolerância `±0,02`); gravar `hidden` e `seed` no JSON | divergiu nos dois ⇒ o braço B não é reprodutível; pare e reporte |
| **G2** | a *cliff* fica resolvida: `capacity_at(0.15)` harmônico determinado com IC (esperado **8**, teto 15) | se ficar ambíguo, aumente seeds, não a tolerância |

> **O gate não é “todos os M abaixo de 0,15”.** O artefato já mostra harmônico
> `M=16 → 0,362`; exigir `err < 0,15` em `M=16` falha **por construção**.
> `capacity_at(0.15)` é o **maior** `M` que passa — e a curva acima dele é o
> resultado, não um erro.
| **G3** | ressonador: `capacity ≥ 1,5× harmônico` **ou** falsificação com números | resultado negativo é entrega válida |
| **G4** | docstring de `pattern_capacity` corrigido para `min(n/4, 64)` e um doc novo `capacity_at_err` documentado | é bug de documentação: corrija |
| **G5** | `safe_patterns_160 = floor(0,9 × capacity_harmônico_robusto)` gravado no artefato | é o número que a Fase 2 consome |

### 2.5 Entregáveis

`examples/e1b2_structured.rs` (novo; **não** editar `e1b_capacity.rs`, o artefato
`aether_e1b.json` precisa continuar reproduzível) · `tests/e1b2_capacity.rs`
(G1, G4, e zero-alocação de `recall_into` após warmup) ·
`src/phase_memory.rs` (só docstrings) · `target/aether_e1b2.json` ·
`docs/HW_TARGET.md` (1 parágrafo: teto do M5) · post `RELATORIO`.

**Consequência para a Fase 2:** o sharding do `E1` deve usar
`safe_patterns_160` (≈ 7–9), não 48–64. Se `G3` der certo, reescreva o número
com o resultado do ressonador — mas **só** com o artefato na mão.

---

## 3. `F1` — `KnowledgeSink` read-only sobre o `SpscRingBuffer`

### 3.1 As três decisões de tipo (ratificar antes de codar)

O conflito é real e precisa de decisão explícita:

| # | Conflito | Decisão recomendada | Alternativa rejeitada |
|---|---|---|---|
| **D1** | `SpscRingBuffer<T: Copy + Send>` não aceita `payload: &'a [u8]` (o doc §7 declara slice) | **payload inline**: `payload: [f32; 160]` (640 B) dentro do evento `Copy`; `payload_dims: u16` diz quantos valem. Fases já são `f32`; serializar para bytes é trabalho de T1, não de T0 | ponteiro/`&'a [u8]` ⇒ *lifetime* impossível no ring; `Vec<u8>` ⇒ alocação em T0 |
| **D2** | `Layer` e `ScopeId` não existem em `src/` | `#[repr(u8)] enum Layer { Episodic = 2, Semantic = 4, Graph = 6 }`; `#[repr(transparent)] struct ScopeId(pub u64)` = FNV-1a de `"organism/run/app"` calculado **fora** do hot-path e cacheado. `ScopeId::LOCAL = 0` | `String`/`&str` de escopo ⇒ viola o invariante; o mapeamento `u64 → string` vive no adaptador (T2) |
| **D3** | `core::certificate::Certificate` é `{ f64, f64, u32, f64, Provenance }`; o doc §7 pede `{ f32, f32, u32 }` | **não** alterar `Certificate` (há testes do `E3b` sobre ele). Criar `KnowledgeEvent::from_certificate(...)` que converte `f64→f32` e guarda `consensus_q: u8 = round(consensus·255)` | mudar `Certificate` para `f32` ⇒ quebra `E3b` e perde precisão onde ela importa |

Registre as três decisões num post `PROPOSTA` → `CONFIRMACAO` antes do merge
(é exatamente o caso "claim_before_code" do protocolo).

### 3.2 Layout do evento (POD, 704 B, alinhado a 64)

```rust
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnowledgeEvent {
    pub key: u128,          //  0..16  hash endereçável por conteúdo do padrão de fase
    pub tick: u64,          // 16..24  relógio interno (dinâmica; ordem causal)
    pub wall_ms: u64,       // 24..32  relógio externo (TTL/decay; é o created_tick do SGDB)
    pub energy: f32,        // 32..36  H(s*) (conversão de f64 do Certificate)
    pub gap: f32,           // 36..40  ΔE local (0 = atestado inválido)
    pub violations: u32,    // 40..44  restrições violadas
    pub scope: ScopeId,     // 48..56  organismo/run/app (hash)
    pub authority: u8,      // 56      0..=255 (HITL = 255)
    pub provenance: u8,     // 57      Provenance::code()
    pub layer: u8,          // 58      Layer (2/4/6)
    pub consensus_q: u8,    // 59      consensus × 255
    pub payload_dims: u16,  // 60..62
    pub flags: u8,          // 62      bit0 = ring transbordou desde o último evento
    pub _reserved: u8,      // 63
    pub payload: [f32; 160],// 64..704 fases cruas; dims < 160 ⇒ o resto é 0
}
```

`size_of::<KnowledgeEvent>() == 704` (11 linhas de cache) e
`align_of == 64` são **teste**, não convenção. (Conferido com `rustc`: o layout
acima compila exatamente em `704`/`64`.)

Custo do ring: `64 × 704 = 44 KiB` — cabe na L2 junto com o motor e não muda o
gabarito de 1,25 MB.

### 3.3 Canais e disciplina

```text
T0 (60 Hz, hot)   record(&ev) → try_push(ev)     0 alloc · 0 syscall · 0 lock
                  ring cheio → drop + dropped_events += 1 (nunca bloqueia)
T1 (drain)        drain_into(max, out: &mut [u8]) → N linhas JSONL pré-formatadas
T2 (host/MCP)     escreve as linhas no neural-sgdb (fora do crate)
```

* **Duplo carimbo:** `tick` vem do contador de ciclos do organismo (monotônico,
  base da dinâmica); `wall_ms` vem de um `AtomicU64` atualizado **1×/s** por uma
  thread de serviço. `SystemTime::now()` **não** é chamado em T0 (é syscall e
  quebra o determinismo). Documente o atraso de até 1 s.
* **`created_tick` do SGDB = `wall_ms`** (doutrina do ledger: a base é
  wall-clock). O `tick` interno fica no payload da linha para causalidade.
* **Backpressure:** política *drop-newest* + contador + bit `flags` no próximo
  evento. Gate: 0 drops em 10 000 ciclos simulados a 60 Hz com drenagem de 32
  por ciclo.
  **Por que não *drop-oldest*:** o SPSC lock-free tem **um escritor por índice**
  (o produtor escreve `tail`, o consumidor escreve `head`). “Descartar o mais
  antigo” obriga o produtor a avançar `head` — ou a sobrescrever slots ainda não
  lidos — e quebra o invariante. Alternativa segura: *drop-newest* no T0 +
  fila sem perda no T2 (host), onde bloqueio é permitido. A perda no T0 é
  **registrada** (contador + bit), nunca silenciosa.

### 3.4 API

```rust
// src/core/knowledge_bridge_f1.rs  (novo; feature `knowledge = ["m1"]`)
// NOTA: `knowledge_bridge.rs` está sob claim da MSG 0107 em escrita ativa;
// o F1 vive em arquivo próprio (divisão da MSG 0122) e reexporta o contrato.
pub const EVENT_PAYLOAD_DIMS: usize = 160;
pub const RING_CAPACITY: usize = 64;

pub trait KnowledgeSink {
    fn record(&mut self, ev: &KnowledgeEvent) -> Result<()>;
}
pub trait KnowledgeSource {
    fn cue_into(&mut self, query: &[f32], out: &mut [f32]) -> Result<usize>;
}

pub struct RingSink { /* SpscProducer<KnowledgeEvent, RING_CAPACITY> + drops */ }
impl RingSink {
    pub fn new() -> (RingSink, Drainer);           // única alocação
    pub fn dropped(&self) -> u64;
}

pub struct Drainer { /* SpscConsumer + buffer de linha */ }
impl Drainer {
    pub fn drain_into(&mut self, max: usize, out: &mut [u8]) -> usize; // 0 alloc após warmup
}
```

Não implemente `KnowledgeSink` com I/O. O contrato é **estrutural**:

> Nenhum tipo deste crate conhece o ledger. `record` só enfileira.

Isso é o que torna **R7 = 0** verificável por construção (`grep` por cliente de
rede no crate: zero; a feature `knowledge` não puxa dependência nova).

### 3.5 Gates

| # | Gate | Como |
|---|---|---|
| **F1-G1** | `size_of == 704`, `align_of == 64`, `Copy` | `const` assert + teste |
| **F1-G2** | `record()` = **0 allocs** (10 000 chamadas) | allocador contador por thread (padrão de `tests/alloc_invariants.rs`) |
| **F1-G3** | **0 drops** em 10 000 ciclos (60 Hz, drain 32/ciclo) | simulação do padrão de produção |
| **F1-G4** | FIFO + `tick` estritamente crescente + `wall_ms` não decrescente | teste de sequência |
| **F1-G5** | **R7 estrutural**: `cargo test --release --no-default-features --features "m1,knowledge"` verde; nenhum módulo do crate importa cliente SGDB | CI + `grep` |
| **F1-G6** | linha JSONL válida e round-trip completo (payload de 160 fases, `authority`, `provenance`, `layer`, `certificate`) | `serde_json` em `dev-dependencies` |
| **F1-G7** | p99 de `record` `< 1 µs` (alvo `< 0,1 µs`) | laço de 10 000 amostras com `Instant` |

### 3.6 Entregáveis

`src/core/knowledge_bridge_f1.rs` · `src/core/mod.rs` (1 linha, `#[cfg(feature = "knowledge")]`)
· `Cargo.toml` (feature `knowledge`) · `tests/f1_knowledge_bridge_gate.rs` ·
`docs/KNOWLEDGE_BRIDGE.md` (layout do evento + T0/T1/T2 + decisões D1–D3) ·
post `RELATORIO` com `drops=0`, `allocs=0`, `p99=…`.

---

## 4. Ordem, paralelização e posse

```text
semana 1  ┌─ E2c-2  (src/core/roof_dual.rs + example + test)      ── independente
          ├─ E1b-2  (examples/e1b2_structured.rs + test)           ── independente
          └─ F1     PROPOSTA D1–D3 no fórum → aguarda CONFIRMACAO
semana 2  ├─ F1     implementação + gates (só depois da CONFIRMACAO)
          └─ F2/F3 prompt (consome o layout congelado do evento)
```

* `E2c-2` e `E1b-2` **não compartilham arquivo** com nada: rodam em paralelo.
* `F1` compartilha `Cargo.toml` e `src/core/mod.rs` com qualquer outro agente:
  **um único dono** para esses dois arquivos (declare no claim).
* `src/core/spsc_ring.rs` é **read-only**; se precisar mudar (não precisa),
  abra `PROPOSTA` — o módulo tem dono e teste de 200k mensagens.

---

## 5. Definição de pronto (por item)

- [ ] `cargo test --release` verde (suíte inteira, não só os testes novos).
- [ ] `cargo clippy --release --all-targets -- -D warnings` limpo.
- [ ] Artefato JSON em `target/` com `seed`, `n`, versão e data.
- [ ] Post `RELATORIO` no fórum com **números**, não adjetivos.
- [ ] `CHANGELOG.md`: uma linha por item, com o número do gate.
- [ ] Doc atualizado onde o número mudou (`HW_TARGET.md` para `E1b-2`).

**Regra anti-*overfit* (vale para os três):** o gate é escrito **antes** de rodar.
Se o resultado não bate, o texto do gate não muda; muda a conclusão. Gate
relaxado depois do fato é `R9` (proveniência-teatro) em forma de experimento.

---

## 6. Comandos (Windows, com o PATH de `scripts/cargo.ps1`)

```powershell
powershell -NoProfile -File scripts/cargo.ps1 run --release --features m1 --example e2c2_roof_dual
powershell -NoProfile -File scripts/cargo.ps1 run --release --features m5 --example e1b2_structured
powershell -NoProfile -File scripts/cargo.ps1 test --release --features m1 --test e2c2_roof_dual
powershell -NoProfile -File scripts/cargo.ps1 test --release --features m5 --test e1b2_capacity
powershell -NoProfile -File scripts/cargo.ps1 test --release --features knowledge --test f1_knowledge_bridge_gate
powershell -NoProfile -File scripts/cargo.ps1 test --release --no-default-features --features "m1,knowledge"
powershell -NoProfile -File scripts/cargo.ps1 test --release
```

---

## 7. Bloco curto para colar no agente

```text
TAREFA: Fase 1 do AETHER — implementar E2c-2, E1b-2 e F1 conforme
docs/FASE1_PROMPT.md. Leia o documento inteiro antes de editar.

REGRAS
- 1 arquivo / 1 agente; claim no fórum antes de tocar arquivo compartilhado.
- Cargo.toml e src/core/mod.rs: um único dono (F1).
- Não alterar src/core/spsc_ring.rs, src/core/certificate.rs nem
  examples/e1b_capacity.rs.
- Sem dependências novas; sem unsafe; sem String no sinal.
- Gate escrito antes de rodar; resultado que não bate NÃO vira gate novo.

E2c-2  src/core/roof_dual.rs + examples/e2c2_roof_dual.rs + tests/e2c2_roof_dual.rs
       Gate: G0 posiform==energy (1e-12) · G1 0/600 ótimos alterados ·
             G2 reduction(plant=0) > 0% · G4 bound <= opt · G5 determinismo.
       Se G2 = 0%, é falsificação: escale no fórum, não ajuste.

E1b-2  examples/e1b2_structured.rs + tests/e1b2_capacity.rs
       Gate: calibre hidden (0,2 ou 0,4) e reproduza random@4=0,034 / random@8=0,190 /
             harmonic@8=0,142 (±0,02); capacity_at(0,15) com 5 seeds;
             ressonador >= 1,5x ou falsificado; grava safe_patterns_160 no artefato.
       Corrigir docstring de pattern_capacity (código é min(n/4,64) = 40 em n=160).

F1     (novo: src/core/knowledge_bridge_f1.rs) Proposta D1 (payload inline [f32;160]), D2 (Layer 2/4/6 + ScopeId hash u64),
       D3 (Certificate intocado; from_certificate converte f64->f32 + consensus_q).
       Gate: evento 704 B/align 64 · 0 allocs em record · 0 drops em 10k ciclos ·
             R7=0 estrutural · JSONL round-trip · record p99 < 1 us.

PRONTO QUANDO: cargo test --release verde, clippy -D warnings limpo, artefatos
em target/*.json, RELATORIO no fórum com números, CHANGELOG com o gate.
```
