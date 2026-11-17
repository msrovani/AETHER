# LanguagePort — por que “LLM de borda” e o que pode substituí-la

> Pergunta: se o AETHER não é um LLM, por que colocar um LLM na borda? Não há
> alternativa? Este documento responde com o estado da arte (arXiv/ACL 2024–2026)
> e transforma a resposta em **portas de linguagem plugáveis**, não em uma
> dependência única.

## 1. Por que a borda LLM apareceu no plano

O buraco do AETHER não é memória, solver ou abstenção — é **linguagem**:
tokenização, fluência, open-domain, código, paráfrase. Nada no núcleo atual
gera texto. Para o agente conversar/entender linguagem livre, alguém precisa
fazer esse trabalho **fora do núcleo** (o núcleo não pode tokenizar).

“LLM de borda” foi um atalho de engenharia:

| Motivo | O que ele compra | O que ele custa |
|---|---|---|
| Qualidade open-domain | fluência, mundo, código | RAM/energia, latência, alucinação |
| Offline/privacidade | nada sai da máquina | modelo grande, updates |
| Latência local | sem RTT de nuvem | prefill/decode caro em CPU pequena |
| Interface única | texto→texto | tokenizer, contexto, custo por token |

**Mas “borda LLM” não é requisito.** É uma escolha entre várias. O plano deve
ter uma **porta** (`LanguagePort`) com backends plugáveis e um default que não
seja o mais caro.

## 2. O que significa “substituir”

Há quatro camadas; substituir uma não substitui as outras:

| Camada | O que faz | Alternativas |
|---|---|---|
| **Tokenização** | texto → unidades | byte/patch (BLT), caracteres, SONAR/sentença |
| **Arquitetura** | sequência → representação | SSM/recurrente (Mamba, RWKV, xLSTM), híbridos |
| **Objetivo** | prever próximo token | prever conceito/sentença (LCM/NCM), JEPA, difusão |
| **Geração** | produzir texto | gramática/template, retrieval extrativo, decoder SONAR |

Um LLM pode ser trocado em qualquer camada; o que **não** existe hoje é uma
alternativa não-neural que iguale open-domain + fluência + código com o mesmo
custo. Há alternativas melhores para **tarefas específicas** (comandos,
estruturas, domínio fechado).

## 3. Famílias de alternativas (com evidência)

### 3.1 Parsing semântico + gramática (a melhor para agente)

**Ideia:** texto → `Goal`/API call estruturada, sem geração livre. Um parser
(regras/gramática) + schema do domínio restringe o espaço de saída.

- **Grammar Pruning** (EMNLP 2025): schema de API + extrator de entidades +
  decoder com gramática podada; >90% execution accuracy em FoodOrdering/
  APIMIXSNIPS/APIMIXATIS, ≥2× menor latência que SOTA; feito para edge.
- **GRAMMAR-LLM** (ACL Findings 2025): gramáticas LL(prefix) para geração
  estruturada; melhora classificação/parsing e validade.
- **RASP** (arXiv:2412.10207): parsing aumentado por recuperação lexical;
  quase dobra OOD em conceitos não vistos.
- **Semantic Parsing with Candidate Expressions** (arXiv:2410.00414): gramática
  com tipos e candidatos da KB; decodificação rápida.

**Encaixe no AETHER:** excelente. O agente já trabalha com `Goal`/`Task`; um
parser + schema transforma linguagem de comando em tool calls **sem gerar
texto**. É o default recomendado para “agente que usa ferramentas”.
**Limite:** não conversa sobre assuntos abertos; precisa de schema/domínio.

### 3.2 Modelos de conceito (LCM/NCM) — o mais alinhado ao AETHER

**Ideia:** em vez de prever o próximo token, prever a **próxima sentença/
conceito** em um espaço de embedding (SONAR), e decodificar com um decoder
frozen.

- **LCM** (Meta, arXiv:2412.08821): 1.6B/7B, prevê embeddings de sentença;
  SONAR 200 línguas; generalização zero-shot multilíngue; usa transformador e
  decoder.
- **Mimir** (arXiv:2605.25263): LCM multilíngue 1.6B; 38,9B sentenças; melhor
  em sumarização longa que Qwen3 1.7B em vários idiomas; ainda sofre com
  generalização de tarefa/repetição.
- **Dynamic LCM** (arXiv:2512.24617): conceitos de tamanho variável aprendidos;
  +2,69% em 12 benchmarks com FLOPs iguais; compressão-aware scaling law.
- **NCM** (arXiv:2610.14162): coarse-to-fine, códigos VQ; 21× mais rápido que
  DLM no LM1B; 10× no OpenWebText.
- **ConceptMoE** (arXiv:2601.21420): funde tokens em conceitos; reduz atenção/
  KV cache; +0,9 ponto em pretraining.
- **Soft Latent Thinking** (arXiv:2608.31069): raciocínio em espaço latente,
  sem head de vocabulário; melhora pass@k em matemática.

**Encaixe no AETHER:** é o caminho mais próximo da premissa contínua. AETHER
já tem memória de fase/FHRR; um LCM-like produz **conceitos** (embeddings) que
podem ser tratados como vetores, com decoder SONAR/template. Não elimina rede
neural, mas elimina **tokenização e geração token-a-token**.
**Limite:** ainda precisa de encoder/decoder (SONAR), e a qualidade de
raciocínio geral é inferior à de LLMs do mesmo tamanho.

### 3.3 Reservoir computing / Echo State Networks

**Ideia:** dinâmica recorrente **fixa** (não treinada) + readout leve. Sem
backprop through time.

- **Reservoir Computing as a Language Model** (arXiv:2507.15779): RC clássico
  chega a test loss 1,81 vs 1,67 do transformer no Shakespeare; AERC 1,73;
  LAERC (reservoir em camadas) escala e substitui atenção por reservatórios
  fixos.
- **Syntactic Learnability of Echo State LMs** (arXiv:2503.01724): ESN grande
  é comparável/superior ao GPT-2 scratch em BLiMP no BabyLM (~100M palavras),
  com menos parâmetros treináveis.

**Encaixe no AETHER:** altíssimo. É a filosofia do motor (dinâmica fixa +
readout). Pode ser um **LM de domínio/edge** barato e determinístico, ou o
mecanismo de “conceito” no LanguagePort.
**Limite:** qualidade open-domain e long-range inferiores; overfit; não é
chatbot.

### 3.4 LMs subquadráticos (Mamba, RWKV, xLSTM, híbridos)

**Ideia:** trocar atenção quadrática por recorrência/SSM; memória constante,
compute linear.

- **xLSTM 7B** (arXiv:2503.13427): comparável a Llama/Mamba de mesmo tamanho,
  maior throughput/prefill e menor memória; “mais rápido/eficiente 7B”.
- **RWKV-7** (arXiv:2503.14456): 1.5B competitivo com Qwen2.5-1.5B/Llama-3.2-1B
  em compressão; bom em recall/associação.
- **Mamba/Mamba-2** (arXiv:2312.00752; estudo 8B arXiv:2406.07887): competitivo
  em zero-shot, mas atrás em MMLU/copying; híbridos (Jamba/Samba/Zamba) ajudam.
- **Gated DeltaNet / híbridos**: alternativas recentes; xLSTM tende a vencer em
  estado-tracking (arXiv:2606.12364).

**Encaixe:** é “LLM de borda” mais eficiente, não uma não-LLM. Útil quando a
qualidade de linguagem é necessária e RAM/energia importam.
**Limite:** ainda tokeniza e alucina; não resolve governança.

### 3.5 Byte Latent Transformer (sem tokenizer)

- **BLT** (arXiv:2412.09871, ACL 2025): bytes → patches dinâmicos por entropia;
  iguala Llama 3 em escala 8B, até 50% menos FLOPs de inferência, robusto a
  ruído/ortografia; sem vocabulário fixo.

**Encaixe:** elimina o tokenizer (alinhado ao “zero tokenização”), mas continua
sendo um LM neural autoregressivo. Pode ser o backend “small LM” do
LanguagePort.
**Limite:** ainda é um LLM; qualidade em MMLU do BLT inicial é baixa sem
inicialização/scale.

### 3.6 Diffusion Language Models (não autoregressivo)

- **LLaDA-8B / Dream-7B** (2025): comparáveis a AR de mesmo tamanho em
  matemática/código; geração paralela e bidirecional.
- **d3LLM** (arXiv:2601.07568): 3,6–5× speedup vs AR 7B; **dInfer**
  (arXiv:2510.08666): >1.100 tok/s em HumanEval com 8×H800; **SlowFast**
  (arXiv:2506.10848): até 15,6–34× speedup.
- **Constrained decoding para DLM** (arXiv:2607.07026): autômato finito garante
  JSON/function calling; BFCL Live Dream 22,4→69,7%.
- **Energia** (arXiv:2610.19855): em 10 tokens AR é ~4× mais eficiente; em 128
  tokens empata; em 1024 tokens difusão é ~2,5× mais eficiente.

**Encaixe:** bom para **planejamento/estrutura/JSON**, onde refinamento
bidirecional e restrições importam; não é default de chat curto.
**Limite:** ainda token-based; qualidade/velocidade dependem do passo de
denoising e do hardware.

### 3.7 Spiking / neuromorphic

- **SpikingMamba-1.3B** (arXiv:2510.04595): 4,76× energia, gap 4,78% (2,23%
  após RL) vs Mamba.
- **TTFS LLM 1.5B** (arXiv:2609.05151): até 1,5B, ANN-level em NLU/raciocínio,
  gap em perplexity; energia estimada por spike count.
- **BiSpikCLM** (arXiv:2605.13859): binário MatMul-free; 4,16–5,87% do custo
  computacional; destilação do ANN.
- **Loihi 2** (arXiv:2503.18002): MatMul-free 370M; 3× throughput, 2× menos
  energia vs edge GPU.
- **Scaling** (arXiv:2610.15146): custo de spike cai com compute; ainda ~0,16–
  0,23 nats atrás.

**Encaixe:** só quando energia é a restrição dura e há hardware neuromórfico.
**Limite:** qualidade/eco-systema imaturos; muitas estimativas são proxy, não
medida.

### 3.8 VSA/HDC — semântica composicional, não geração

- **Hyperdimensional Probe** (arXiv:2509.25045): VSA decodifica representações
  de LLM; conceitos interpretáveis.
- **Attention as Binding** (arXiv:2512.14709): atenção ≈ VSA; propõe binding/
  unbinding explícito e memória hiperdimensional.
- **VSA-Lisp com aritmética residual** (arXiv:2511.08767): Lisp Turing-completo
  em espaço vetorial; interpretador VSA.
- **HDC review** (Springer 2025): survey comparativo.

**Encaixe:** é o que o AETHER já faz (FHRR). Serve para **memória, binding,
composição e interpretabilidade**, não para gerar texto fluente. É uma camada
do LanguagePort, não o LanguagePort inteiro.

### 3.9 World models / JEPA

- **V-JEPA 2** (arXiv:2506.09985): prevê em espaço latente; planejamento
  zero-shot com imagem; encoder sem linguagem alinhado a LLM.
- **LLM-JEPA** (arXiv:2509.14252): objetivo JEPA junto do NLL; melhora
  abstração mantendo geração.
- **VL-JEPA** (2025): prevê embedding da resposta e só decodifica quando
  necessário; 50% menos parâmetros treináveis e 2,85× menos operações de
  decoding (relatado).

**Encaixe:** pode ser o **backbone de entendimento/predição** do AETHER; não
resolve verbalização sozinho. Combina com LCM/conceito.

### 3.10 Sem geração: retrieval + template

Para respostas auditáveis, a alternativa mais segura é **não gerar**: recuperar
fato/trecho do SGDB e montar resposta com template/gramática. Custo mínimo,
zero alucinação, mas sem fluência geral. É o default para “cite/abstain”.

## 4. Matriz de decisão para o AETHER

| Necessidade | Default | Alternativa | LLM necessário? |
|---|---|---|---|
| Comando/tool call | parser + schema/gramática | SLM com gramática podada | não |
| Pergunta factual | memória + template/extrativo | concept decoder | não |
| Explicação curta | template + fatos | small LM | talvez |
| Conversa aberta | remote LLM | small LM (xLSTM/RWKV/BLT) | sim |
| Código | remote LLM | small code LM | sim |
| Planejamento estruturado | solver + diffusion constrained | LLM | não/sim |
| Domínio fechado/edge | ESN/LAERC | parser/grammar | não |
| Energia extrema | regras/template | spiking | não (mas hardware) |

## 5. Recomendação: `LanguagePort` com 5 backends

```rust
pub trait LanguagePort: Send {
    fn parse(&mut self, text: &str, schema: &GoalSchema) -> Result<Goal>;
    fn generate(&mut self, concepts: &[Concept], style: Style) -> Result<String>;
    fn capabilities(&self) -> LanguageCaps;  // custo, risco, latência
}
```

| Backend | Uso | Dependência | Status |
|---|---|---|---|
| `parser` | comandos/estruturas | gramática/schema | **default recomendado** |
| `template` | respostas factuais/auditáveis | SGDB + template | default sem geração |
| `small_lm` | linguagem local | xLSTM/RWKV/Mamba/BLT 0.5–3B | opcional |
| `concept` | conceito→texto | SONAR/LCM-like + decoder | pesquisa (B/C) |
| `remote_llm` | open-domain/código | API | fallback caro |
| `spiking` | energia | neuromórfico | pesquisa |

O **planner** escolhe o backend por `EFE + custo + risco + privacidade`, não por
“tem LLM?”. O AETHER continua funcionando com `parser`/`template` mesmo sem
nenhum LLM.

## 6. O que ainda **não** dá para substituir

- Conversa aberta fluente e generalista.
- Conhecimento de mundo amplo e atual.
- Geração de código arbitrário.
- Instrução complexa em linguagem natural sem schema.

Para esses casos, o LLM (remoto ou pequeno) continua sendo a opção pragmática.
A diferença é que ele deixa de ser **o centro** e passa a ser **uma tool** com
custo, risco e validação — exatamente o que o plano híbrido já diz.

## 7. Experimentos baratos antes de escolher (gates)

| ID | Experimento | Métrica | Decisão |
|---|---|---|---|
| **L1** | Grammar Pruning/parser em 100 comandos do agente | exact match/slot F1, p99 | default se ≥95% e <10 ms |
| **L2** | xLSTM/RWKV/Mamba/BLT 1–3B em 50 prompts | qualidade, RAM, tok/s | small_lm se couber no budget |
| **L3** | LCM/Mimir-like com SONAR em sumarização/comandos | similaridade semântica, decoder latency | concept se ≥0,9 e <50 ms |
| **L4** | ESN/LAERC em corpus de domínio | perplexity/BLiMP, p99 | edge se ≥ baseline local |
| **L5** | Diffusion constrained para JSON/tool call | validade, latência | planning se >95% válido |
| **L6** | Spiking/neuromórfico | energia real (não proxy), qualidade | só com hardware |

## 8. ADR-013 — Linguagem é uma porta, não uma dependência

**Decisão:** `LanguagePort` com backends plugáveis; default `parser` +
`template`; `small_lm`/`concept`/`remote_llm`/`spiking` opcionais e escolhidos
pelo planner. Nenhum backend no `T0`; nenhum texto no núcleo.

**Consequências:** o AETHER não fica refém de um LLM; a qualidade open-domain
continua exigindo LLM; a pesquisa de concept/ESN pode avançar sem bloquear o
agente.

## 9. Referências (para ler/rodar)

- LCM: arXiv:2412.08821 · Mimir: arXiv:2605.25263 · DLCM: arXiv:2512.24617
- NCM: arXiv:2610.14162 · ConceptMoE: arXiv:2601.21420 · Soft Latent Thinking: arXiv:2608.31069
- BLT: arXiv:2412.09871
- xLSTM 7B: arXiv:2503.13427 · RWKV-7: arXiv:2503.14456 · Mamba: arXiv:2312.00752 · Mamba 8B: arXiv:2406.07887
- Diffusion survey: arXiv:2508.10875 · d3LLM: arXiv:2601.07568 · dInfer: arXiv:2510.08666 · constrained: arXiv:2607.07026 · energy: arXiv:2610.19855
- Reservoir LM: arXiv:2507.15779 · ESN syntax: arXiv:2503.01724
- SpikingMamba: arXiv:2510.04595 · TTFS: arXiv:2609.05151 · BiSpikCLM: arXiv:2605.13859 · Loihi 2: arXiv:2503.18002 · scaling: arXiv:2610.15146
- VSA: arXiv:2509.25045 · Attention as Binding: arXiv:2512.14709 · VSA-Lisp: arXiv:2511.08767
- JEPA: V-JEPA 2 arXiv:2506.09985 · LLM-JEPA arXiv:2509.14252
- Parsing: Grammar Pruning (EMNLP 2025) · GRAMMAR-LLM (ACL Findings 2025) · RASP arXiv:2412.10207 · candidate expressions arXiv:2410.00414
