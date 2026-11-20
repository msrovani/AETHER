# Análise vega-U — candidato a decisor do AETHER

Data: 2026-11-19 · Autoria: agente OPENCODE (borda LLM), fontes públicas citadas
Status: análise de adoção, SEM integração de código. Gate de adoção pendente (F0/F1).

## Fontes

- Repo: github.com/NandhaKishorM/vegaml (Apache-2.0; 311 stars; vegaml 0.16.3, 2026-11-15)
- Método/caveats: docs/vega-u.md no repo (medido out/2026)
- Blog do método: embedding.in/blog/vega-u-one-engine-any-reader
- Paper: "From Laya to Vega: Open Non-Autoregressive Decision Models with
  Calibrated Probabilities" — Preprints 2026, 2026110781 (PREPRINT, sem peer review)
- Terceiros: systemonemodels.org (Jev Decision Index), blog.mornati.net (VRAM/instalação),
  carouter.ai (benchmark EN/FR com Kev/Laya/Jev), berget.ai (latência/suites independentes)

## O que é

Decisor não-autoregressivo: **LM congelado (percepção) + porto linear + motor de decisão
de 14.3M params (57MB) num espaço compartilhado de 1024 dims**. Nenhum texto é gerado.
Pipeline por decisão:

1. Um prompt formatado: `Observation: {state} Measurement ({type}): {instr} Possible outcomes: * ...`
2. UMA passada do reader congelado; pooling de 5 spans (situação, pergunta, um span por
   opção, token final)
3. Vetores viram condição inicial: partícula (z0, p0) num espaço de decisão de 64 dims
4. Cada opção candidata vira um poço gaussiano U(z) = ½κ‖z‖² − Σ a_k·exp(−‖z−c_k‖²/2σ_k²)
5. Dinâmica hamiltoniana amortecida (symplectic Euler + termostato), custo constante,
   determinístico; saída cedo quando assenta
6. Resposta = poço de menor energia; P = softmax(−E/τ)

**Calibração física** (o diferencial): a temperatura é prevista POR DECISÃO a partir do
estado final da partícula — log τ = b + w·[log(1+energia cinética residual), log(1+distância
ao poço mais próximo), fração do orçamento de passos usada, log(nº de opções)]. Partícula
em movimento ou longe de todo poço → temperatura mais quente → probabilidade honesta.

## Flags de saída (gêmeas da doutrina AETHER)

| Flag vega-U | Equivalente AETHER |
|---|---|
| `abstain` (abaixo do piso de confiança ajustado) | `Action::Abstain` |
| `unbound` (partícula assentou longe de todo poço = OOD) | gate de abstenção OOD (OOD<=0.49) |
| conformal prediction set (risco escolhido) | banda de abstenção com cobertura |
| `readout` no campo (engine vs ttt head) | proveniência/certificado |
| routing `auto`/`review` com motivo por resposta | escalada HITL |

## Números medidos (out/2026, engine byte-idêntico entre readers)

| Reader | Como | Held-out acc | Zero-shot acc | ECE | AURC | acc@50% cov |
|---|---|---|---|---|---|---|
| Qwen3.8-27B | attached cold | **0.811** | 0.629 | 0.021 | **0.053** | **0.976** |
| Qwen3.5-4B | attached cold | 0.791 | 0.568 | 0.017 | 0.063 | 0.965 |
| Qwen3.5-0.8B | treinado junto | 0.765 | 0.534 | **0.010** | 0.073 | 0.956 |
| gemma-3-1b-it | treinado junto | 0.738 | 0.504 | 0.007 | 0.093 | 0.927 |

A inversão é o resultado-chave: leitores **cold-attached** (nunca vistos no treino)
pontuaram ACIMA dos treinados — o motor aprendeu a tarefa, não o reader. Ganhos do 27B
caem onde a leitura importa (70k tokens 0.831→0.941; ARC 0.647→0.922) e tarefas saturadas
ficam planas (~0.99 phishing/spam/intents). O tipo `score` (ordinal) trava em ~0.61 para
TODOS os readers — teto do engine/labels, não do reader.

## vega-U vs concorrentes (para o nosso caso: RTX 3050 6GB, PT/EN, Apache-2.0)

- **Jev 1.14 (hosted)**: melhor zero-shot (0.857 OOD, índice 60.24) — fechado, metered,
  dados saem da rede: viola o pilar de privacidade do LanguagePort
- **Drex 2.0 (Qwen3.6-35B MoE)**: ≈Jev, self-host ~37GB Q8, licença RAIL-M (restrições):
  não cabe, licença restritiva
- **Kev-0.8B** (LoRA+pointer sobre Qwen3.5-0.8B): melhor zero-shot que cabe (0.70 EN /
  0.64 FR no CArouter; 0% de erro confiante; "any 4GB GPU") — MAS inglês-only e
  decoder-based; calibração válida só in-distribution
- **Laya (encoders)**: rápido (33ms), multilingual 45/51 idiomas — mas zero-shot quase
  aleatório no Jev Decision Index (9.21) e overconfident (relatos Berget/SOTAAZ)
- **vega-U**: zero-shot modesto (0.534 @0.8B) mas ECE 0.010 e **acc@50% cobertura 0.956**
  — a métrica certa para quem "só age quando tem certeza". E o `attach` local permite
  plugar qualquer reader (até o nosso Qwen local) sem treinar nada

## Cabimento no hardware (RTX 3050 Laptop 6GB)

- 0.8B reader ~1.77GB + engine 57MB; CUDA usa bf16/fp16 por capability → ~2-3GB total:
  **cabe com folga**; CPU/MPS funcionam (fp32, mais lento)
- O 4B (9.34GB) NÃO cabe em 6GB de VRAM (vira CPU/swap); o 27B exige 80GB
- vegaml **já tem servidor MCP** (`vegaml[mcp]` → `vegaml-mcp-server`: decide() como tool
  com gate) — mesma arquitetura de integração que já usamos com o neural-sgdb
- laya ≥0.4.23 embute o vegaml com Router: **texto não-inglês (PT) e estados longos vão
  para a lane vega automaticamente**

## Caminho de adoção (gate da casa)

1. **F0 probe**: `pip install vegaml`, rodar no 3050 com o 0.8B; medir VRAM/latência
   reais neste host → fato no nsgdb
2. **F1 corpus rotulado**: extrair decisões com desfecho do FORUM.txt + nsgdb no formato
   `[state, {question: label}]` do `vegaml.calibrate` (o formato é literalmente o nosso
   ciclo: proposta→desfecho medido). ~500 exemplos por tipo de pergunta para ser usable;
   1-2k onde assenta
3. **F1 gate**: medir acurácia/abstenção/error confiante no held-out; tetos explícitos
   (metodologia do gate CIC: false_decision com intervalo de Wilson); abaixo do teto →
   escalar HITL, nunca agir
4. **F2 assentos** (só após gate verde): (a) arbitragem de conflito nsgdb — `choice` entre
   versões + `audit_decision` hash-chain; (b) triagem de quarentena — `noul` "passou na
   verificação?"; (c) escalada do chat `Ambiguous`
5. **F3**: `FROM: LAYA/VEGA` no fórum para desempate, sempre com o score na MSG

## Caveats honestos

- Preprint sem peer review; comunidade jovem (311 stars); benchmarks do próprio vendor
  (mitigado: `eval_report.json` por reader + harness aberto rerunnable em Kaggle)
- Zero-shot do 0.8B é MEDIANO (0.534); o valor vem do uso **selectivo** (acc@50%cov 0.956)
  e da calibração com NOSSOS rótulos — porto/calibração ajustados no nosso corpus invalidam
  os números publicados (revalidar tudo localmente)
- Tipo `score` é fraco (~0.61): decisões ordinais (urgência) devem ir ao HITL ou virar
  `choice` de baixa cardinalidade
- O "physics engine" é mecanismo de inferência, não modelo do mundo — os próprios autores
  dizem que o espaço de 64 dims não tem referente físico
- Treino do engine: 708.657 itens até 73.728 tokens — nós NÃO reproduzimos isso; só
  consumimos o checkpoint e calibramos (attach/calibrate são locais e baratos)
- fp32 em CPU/MPS; no Windows lembrar do fix de UTF-8 locale (0.12.3 já corrigido no pacote)

## Veredito

Melhor candidato **tecnicamente alinhado** à doutrina do AETHER (abstenção nativa,
calibração auditável, conformal, proveniência por resposta, Apache-2.0, self-host, MCP
nativo). Não é melhor que Jev em acurácia bruta — mas Jev viola privacidade/custo, e o
uso selectivo do vega (act só acima do teto medido) é exatamente a semântica
`act/cite/propose/abstain` que o core já executa. Gate F0/F1 decide com número.
