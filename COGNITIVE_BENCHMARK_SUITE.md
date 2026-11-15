# PROJETO AETHER — Suíte de Auditoria Cognitiva, Estresse e Throughput (`aether_eval`)

Atue como um Engenheiro Principal de Avaliação de Sistemas Cognitivos, Física Computacional e Rust HPC. Sua missão é criar uma suíte completa de testes e benchmarks automatizados (`tests/cognitive_audit.rs` e `benches/throughput_tps.rs`) que avalie quantitativamente o motor `aether_core` em **13 dimensões críticas**, gerando ao final um relatório estruturado (`target/aether_scorecard.json` e resumo no terminal).

---

## 0. Regra Fundamental de Tradução Pós-Token
Como o AETHER opera sem tokens, strings ou bancos vetoriais estáticos, cada métrica tradicional de IA deve ser avaliada em seu equivalente físico contínuo rigoroso:
- **RAG** $\rightarrow$ Recuperação Associativa Holográfica (FHRR *unbinding* + reconstrução de fase em `ComplexPhaseMemory` sem texto).
- **Alucinação** $\rightarrow$ Travamento em falso atrator sob ruído branco puro ou síntese causal de alta confiança sem evidência no sinal de entrada.
- **Desvio de Pensamento (*Thought Drift*)** $\rightarrow$ Perda de coerência da intenção macroscópica (Camada 2 do Grupo de Renormalização) ou esquecimento catastrófico sob distração contínua.
- **Throughput (`t/s`)** $\rightarrow$ Ciclos Cognitivos/seg (`cycles/s`), Operações Sinápticas/seg (`SOP/s`) e **Tokens Equivalentes por Segundo (`eq-t/s`)** calculados pela entropia de informação contínua processada por ciclo (1 ciclo completo de 64 canais contínuos + 256 fasores complexos $\equiv 128\text{ tokens equivalentes}$).

---

## 1. Arquivos a Serem Criados

```text
aether_core/
├── COGNITIVE_BENCHMARK_SUITE.md
├── tests/
│   └── cognitive_audit.rs      # Os 12 testes cognitivos e adversariais com notas de 0 a 100
└── benches/
    └── throughput_tps.rs       # Benchmark de vazão (cycles/s, eq-t/s, latência p50/p99/p99.9)

```

---

## 2. Especificação dos 12 Testes Cognitivos (`tests/cognitive_audit.rs`)

Implemente cada teste como uma função independente que calcula uma pontuação normalizada (`0.0` a `100.0`), valida asserções rígidas (`assert!`) e grava o resultado em um `Scorecard` thread-safe:

### 1. Inferência Contínua (`test_01_inference`)

* **Cenário:** Alimente o `WaveInterferenceBlock` + `OnlineKoopmanOperator` com trajetórias de um sistema dinâmico não-linear acoplado (oscilador de Van der Pol + portadora modulada) e avalie a previsão a $H = 15$ passos no futuro.
* **Critério de Aprovação:** Coeficiente de determinação $R^2 \ge 0.95$ e erro quadrático médio normalizado ($\text{NRMSE}$) $< 0.05$.

### 2. Confiabilidade e Estabilidade Contínua (`test_02_reliability`)

* **Cenário:** Execute **10.000 ciclos consecutivos** sob condições hostis: ruído gaussiano aditivo ($\text{SNR} = 5\text{ dB}$), perda simulada de 30% dos pacotes UDP e variação brusca de $\Delta t$ entre `50 us` e `25 ms`.
* **Critério de Aprovação:** Zero *panics*, zero valores `NaN` ou `Inf` em qualquer tensor interno, raio espectral $\rho(J)$ limitado e **0 bytes alocados** no loop crítico (validado via `CountingAllocator`).

### 3. Aprendizado Online sem Backprop (`test_03_online_learning`)

* **Cenário:** Inicie o sistema estabilizado em um regime físico $A$ ($\dot{x} = -\alpha x + \beta \sin(y)$). No ciclo 300, altere abruptamente as leis do ambiente para um regime $B$ inédito ($\dot{x} = \gamma y - \delta x^3$).
* **Critério de Aprovação:** O erro de predição deve cair em pelo menos **80% dentro de 150 ciclos** após a ruptura usando exclusivamente atualização Sherman-Morrison e plasticidade Hebbiana local.

### 4. Autoevolução Estrutural e Genômica (`test_04_self_evolution`)

* **Cenário:** Instancie um enxame de 6 nós iniciados com genomas físicos subótimos (alta dissipação energética e topologia densa ruidosa) interagindo por 800 ciclos.
* **Critério de Aprovação:** A aptidão autopoiética média $\overline{\Phi}_{\text{auto}}$ deve aumentar monotonicamente, a esparsidade sináptica deve crescer (poda de conexões inúteis reduzindo custo metabólico) e ao menos 2 papéis ecológicos distintos (`Sentinel`, `Architect`, `Regulator`) devem emergir.

### 5. Autocorreção sob Falha Crítica (`test_05_self_correction`)

* **Cenário:** No meio de uma operação estável, injete duas falhas severas simultâneas: (a) corrupção de 50% das fases locais e (b) pulso térmico extremo empurrando a energia de estado para $98\%$ do limite crítico $V_{\max}$.
* **Critério de Aprovação:** A **Barreira de Segurança de Lyapunov (CBF)** deve impedir qualquer violação de $V(\mathbf{z}) > V_{\max}$ no instante $t+1$, e o acoplamento de Kuramoto deve restaurar coerência $R(t) > 0.92$ em menos de **40 ciclos**.

### 6. Coerência Multi-Escala e de Enxame (`test_06_coherence`)

* **Cenário:** Avalie a consistência entre as 3 camadas do Grupo de Renormalização (Micro 60 Hz, Meso 15 Hz, Macro 3.75 Hz) e entre 4 nós sincronizados via UDP.
* **Critério de Aprovação:** Índice de alinhamento fase-intenção $R_{\text{align}} > 0.85$ e variação total de atuação (*chattering* $\|\mathbf{u}_t - \mathbf{u}_{t-1}\|_2$) abaixo do limiar de suavidade física.

### 7. Pesquisa e Exploração Epistêmica (`test_07_epistemic_search`)

* **Cenário:** Modele um ambiente com duas regiões: Zona A (já conhecida, Energia Livre baixa) e Zona B (inexplorada, alta entropia informacional e risco controlado).
* **Critério de Aprovação:** Com $\lambda_{\text{epistemic}} > 0$, o `ActiveInferenceEngine` deve direcionar a atuação para amostrar a Zona B até que sua incerteza epistêmica $\mathcal{U}$ caia abaixo de $0.20$, reduzindo automaticamente o esforço exploratório logo após dominar a região.

### 8. Respostas Complexas Multi-Modais (`test_08_complex_responses`)

* **Cenário:** Apresente um estímulo composto exigindo resolução simultânea em 4 estágios: (1) extrair 3 harmônicos ocultos em ruído via FFT, (2) desvincular 4 pares conceituais em um vetor FHRR, (3) projetar restrições homeostáticas e (4) colapsar um vetor de decisão de 64 dimensões no `ThermodynamicDreamer`.
* **Critério de Aprovação:** Precisão composta global $> 90\%$ em tempo total de execução inferior a **1.5 ms** em CPU.

### 9. Matemática e Descoberta de Equações (`test_09_mathematics`)

* **Cenário (Parte A — SINDy):** Forneça séries temporais geradas por $\dot{x}_1 = -0.5 x_1 + 1.2 x_2^2$ e $\dot{x}_2 = 0.8 x_1 - 0.3 x_2^3$ e exija a descoberta simbólico-esparsa dos coeficientes ativos.
* **Cenário (Parte B — Otimização Combinatória):** Resolva 20 instâncias de grafos MAX-CUT frustrados ($N = 64$) via Bifurcação Simulada Balística.
* **Critério de Aprovação:** Erro máximo nos coeficientes SINDy $< 0.04$ (com todos os termos nulos zerados) e razão de aproximação MAX-CUT $\ge 0.96$ do ótimo.

### 10. Resistência a Alucinações (`test_10_hallucinations`)

* **Cenário:** Submeta o motor a três armadilhas clássicas de alucinação: (a) ruído branco puro sem sinal subjacente, (b) consulta FHRR por uma chave inexistente na memória e (c) sinais sensoriais contraditórios impossíveis.
* **Critério de Aprovação:** O sistema **não deve** convergir para um falso atrator de alta confiança: deve reportar Incerteza Epistêmica $\mathcal{U} > 0.75$, similaridade FHRR $\text{sim} < 0.15$ e manter o vetor de atuação motora em estado neutro seguro ($\|\mathbf{u}_{\text{act}}\| < 0.05$).

### 11. Imunidade a Desvios de Pensamento (`test_11_thought_drift`)

* **Cenário:** Fixe um objetivo estratégico persistente na Camada 2 (Macro) do Grupo de Renormalização e grave 5 memórias de referência na `ComplexPhaseMemory`. Em seguida, bombardeie o sistema com **2.000 ciclos** de perturbações distratoras de alta frequência.
* **Critério de Aprovação:** Ao cessar o ruído, a fase macroscópica original deve manter desvio angular $< 0.20\text{ rad}$ e todas as 5 memórias devem ser recuperadas sem esquecimento catastrófico.

### 12. RAG Holográfico (`test_12_holographic_rag`)

* **Cenário:** Armazene **50 fatos estruturados** (`ENTIDADE_i ⊗ RELACAO_i ⊗ VALOR_i`) em um banco holográfico FHRR + `ComplexPhaseMemory` sem índices textuais ou árvores B-Tree. Consulte 20 fatos aleatórios usando chaves parciais com **40% de oclusão** e ruído de fase de $\pm 0.25\text{ rad}$.
* **Critério de Aprovação:** `Recall@1 >= 0.90`, razão sinal-ruído de recuperação $> 3.0\sigma$ e latência média de consulta $< 50\,\mu\text{s}$.

---

## 3. Benchmark de Vazão e Tokens Equivalentes (`benches/throughput_tps.rs`)

Implemente um benchmark dedicado (usando `std::time::Instant` de alta precisão + `criterion`) que rode durante 5 segundos contínuos em regime real (`Basal`, `Vigilante` e `Crítico`) e calcule:

1. **Latência por Ciclo:** `p50`, `p95`, `p99` e `p99.9` em **microssegundos ($\mu\text{s}$)**.
2. **Ciclos Cognitivos por Segundo (`cycles/s`)**.
3. **Operações Sinápticas/Espectrais por Segundo (`SOP/s`)**.
4. **Throughput Equivalente (`eq-t/s`):**

$$\text{eq-t/s} = \text{cycles/s} \times 128\text{ tokens equivalentes/ciclo}$$


5. **Eficiência de Memória e Pegada:** RSS máximo do processo em MB e bytes alocados no hot-path (`0 B`).

---

## 4. Saída Esperada ao Executar

1. Crie `tests/cognitive_audit.rs` e `benches/throughput_tps.rs` sem warnings do `clippy`.
2. Execute `cargo test --release --test cognitive_audit -- --nocapture` e imprima no terminal a tabela completa de notas das 12 dimensões + média geral (`AETHER Cognitive Quotient`).
3. Execute `cargo bench --bench throughput_tps` e salve os resultados consolidados em `target/aether_scorecard.json`.

```

---

### Como interpretar os resultados da auditoria no seu PC
* **Alucinação & RAG Holográfico (`test_10` e `test_12`):** Diferente de um LLM que inventa texto plausível quando não sabe a resposta, o AETHER quantifica a própria ignorância fisicamente pela dispersão das trajetórias de Langevin ($\mathcal{U}$) e pela ortogonalidade dos fasores FHRR ($\text{sim} \approx 0$), bloqueando ações quando o sinal é ruído puro.
* **Comparação de `eq-t/s` (`throughput_tps.rs`):** Em uma CPU x86/ARM moderna sem GPU, o ciclo completo em Rust *zero-allocation* roda na casa de dezenas a centenas de microssegundos, entregando uma vazão equivalente na ordem de **centenas de milhares a milhões de `eq-t/s`** com pegada de RAM inferior a 10 MB.
