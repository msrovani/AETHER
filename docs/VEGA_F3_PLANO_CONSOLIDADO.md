# Plano Consolidado F2 → F3 — Otimização Total do Híbrido

Data: 2026-11-20 · Status: **APROVADO por HITL** (fórum + nsgdb, autoridade 255)
Este documento é o contrato do ciclo de build que se segue. Nada entra em
execução sem passar pelas portas aqui definidas.

## Decisões HITL (irreversíveis no ciclo)

| Decisão | Escolha |
|---|---|
| Integração Rust ↔ Python (AND gate) | **FFI PyO3** (não HTTP serve) |
| Vetor de pesquisa externa (abstenção `unknown`) | **Híbrido**: websearch do harness agora, tool própria no `aether-agent` na F5 |
| AND gate F2 | Aprovado em MSG 0756 → executado em MSG 0758 |

## Os 6 eixos transversais (aplicados em TODAS as fases)

### A — Fiação
- Crate `aether-decision`: binding PyO3 de `vegaml`; `load()` + `fit(124)` no
  start da thread; `DecisionPort` trait no chat; GIL liberado durante o forward
  (`Python::allow_threads`).
- Bootstrap: 1–2 min, determinístico, na subida do daemon (lazy + warmup thread).

### B — Canos: dimensões dos dados
- Estado → reader: **digest de certificado ≤320 tokens**, nunca payload bruto.
- Resposta do gate: schema JSON tipado e estável
  `{answers:[{choice,probs,abstain,readout,motivo}], conformal_set, meta}`
  (pydantic / vegaml[structured] — contrato espelhado no Rust).
- Escrita nsgdb: campos de proveniência estruturados (key, authority, scope),
  append-only L3 — não prosa.
- Corpus de rótulos: 124 → **500/tipo** (choice 56→500, boolean 68→500).
- Embedder: 384-dim MiniLM (hit@5 0.974 vs 0.950 lexical).

### C — Tomadas: conexões externas enumeradas
1. FFI PyO3 — o único ponto de contato Rust↔Python (in-process, porta 0)
2. **Vetor de pesquisa externa** (tomada nova da abstenção `unknown`)
3. `embed_server.py` — memória semântica interna
4. Outbox `AETHER_SGDB_OUTBOX` — fatos sobrevivem a queda do banco
5. `HF_HUB_OFFLINE=1` — padrão: **nenhuma chamada externa por default**;
   socket fora do inventário é bug.

### D — Azeite (motores)
- Feito: kernels Triton GDN (FLA 0.5.0 + triton-windows 3.8.0, sem MSVC) —
  186.3 → 116.2ms p50 (−37%).
- Próximo: torch.compile condicional (só mantém se ≥20% medido); read-once
  prefix caching; bf16 (ativo); warmup fora da medição; ambos readers no mesmo
  processo (~4–4.5GB de 6GB).
- Protocolo: re-probe n=30 antes/depois de cada mudança. Ganho medido ou não vale.

### E — Isolamento dos pontos de contato
- Trait `DecisionPort` = única face do vega no Rust; troca de implementação não
  toca o chat.
- **Fronteira de quarentena**: evidência externa entra só em 60–120; só HITL
  sanciona 255.
- PyO3 é in-process ⇒ compensações obrigatórias:
  - torch 2.14.1+cu126 + python pinados no lockfile (upgrade = rebuild + re-probe)
  - watchdog VRAM: >5GB → `Abstain{reason:"engine_unavailable"}` — nunca trava
  - exceção Python → abstain tipado, nunca panic para cima do chat

### F — Confiabilidade da pesquisa externa (aprovado com os exemplos)

**Camada 0 — tier de fonte (antes da agregação):**
```
tier: primária (paper/docs/repos) > secundária > agregador > SEO/llm-farm
autoridade_final = min(teto_do_tier, score_de_convergência) − penalidade_independência
```
O `min()` protege: fonte ruim convergida NÃO sobe (3 blogs copiados = 1 origem).

**Camada 1 — convergência:**
- numérica: extrair valores → σ/μ < 5% = convergiu
- semântica: acordo vs conflito entre fontes
- independência real: paráfrase/cópia conta como 1

**Camada 2 — veredito:**
```
settled (convergiu)            → autoridade 120  "conhecimento adquirido"
convergiu fraco                → 80–100          "adquirido, rotular com cautela"
open (conflito)                → 60              "dúvida MANTIDA"
pergunta irreductível (open)   → 60 SEMPRE, mesmo com fontes concordando
```
Nunca 255 — HITL sanciona. Pesquisa prepara o rótulo, não aprova.

**Distinção crucial — quem faz o quê:**
| Camada | Quem | Papel |
|---|---|---|
| Etiqueta inicial da pergunta | Borda LLM | bootstrap até existirem rótulos |
| Classificação madura | head `question_kind` do vega-U | flywheel F3, CV, descarte honesto |
| Evidência/conflito | convergência (Eixo F) | mede, não decide — core não decide |
| Sancionar | HITL | único caminho 60→255 |

Convergência detecta divergência; NÃO distingue "resposta errada" de "pergunta
sem resposta única" — essa é propriedade da PERGUNTA, decidida a priori.

**Exemplos de régua (casos de teste):**
- "Distância Terra–Lua" → 384.400km σ/μ≈0.01% → settled 120 → candidato a fato
- "Rust é melhor que C?" → debate irreductível → open 60, mapa adquirido,
  nunca classe vencedora
- 3 blogs idênticos → independência detecta cópia → 1 fonte → 60 ou descarte

**Camada 3 — ampliação (comando HITL sobre o vetor):**
```
ampliar: +fontes +tiers +sinônimos PT/EN +datas → re-roda tripla → delta registrado
persistiu conflito após ampliar → open DEFINITIVO (dúvida resistiu ao esforço)
convergiu após ampliar          → settled com marca "resolvido por ampliação"
```
- Ampliar nunca sobe o teto do tier; melhora convergência, não transforma fórum
  em paper.
- Persistência do conflito é informação → vira rótulo de qualidade para
  `question_kind` (perguntas que resistem à ampliação são genuinamente abertas).
- Sistema NUNCA amplia sozinho — comando explícito de quem sanciona.

## Fases (sequência de build)

```
F1: crate PyO3 — DecisionPort + pin de versão + watchdog VRAM + warmup
F2: taxonomia de abstention (unknown|ambiguous|conflict)
    → tomada de pesquisa em unknown → tripla (evidência, convergência, veredito)
    → quarentena 60–120
F2.5: camada de tier de fonte + comando "ampliar" (HITL)
F3: flywheel 124 → 500/tipo (+ rótulos de question_kind + ampliação como sinal)
F4: embed_server MiniLM no nsgdb (384-dim)
F5: API do híbrido + tool própria de pesquisa (contrato: tier+independence por
    fonte) + eval tiers T0–T3 contínuos
```

## Critérios de aceite

| Fase | Aceite |
|---|---|
| F1 | primeira decisão via DecisionPort; watchdog dispara em teste de VRAM; exceção Python vira abstain tipado |
| F2 | pergunta fora do léxico dispara pesquisa → resposta citada com veredito → grava em quarentena |
| F2.5 | fontes copiadas contam como 1; ampliar registrado como comando com delta |
| F3 | head `passou` vence engine em CV; `question_kind` etiqueta factual/open |
| F4 | hit@5 ≥0.974 no corpus do projeto |
| F5 | T0–T3 rodando a cada mudança |

## Estado herdado (linha de partida)

- 176 testes lib, 0 falhas; clippy/fmt limpos; auditoria cognitiva 12/12 MEASURED
- vega-U 0.8B calibrado: ECE 0.0739; T1 0.97 age; latência 116.2ms p50
- gemma-prose sentinela: T2 abstain ok; C1 winner head A 0.916; 137.8ms
- C3 resolvida: route head 0.97+ (injeção→escalar 0.966)
- C2 conservadora (head `passou` descartado honesto) — alvo da F3
- Corpus: 124 rótulos reais (`data/vega_calibration_starter.json`)
- Registro: nsgdb `1795133649609000`, `1795181417877000`, `1795182168337000`
  (autoridade 255); fórum MSG 0756–0758
