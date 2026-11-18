# Pesquisa externa — Verify / authority 200 (2026-11-18)

Questão: wire de `verify` → autoridade 200 sem LLM externo no T0.

## Literatura

| Paper | Ideia | Link |
|---|---|---|
| ToolVerifier | Self-verify seleção de tool + parâmetros | https://arxiv.org/abs/2402.14158 |
| CaLM | LM pequeno valida resposta só com fontes citadas | https://arxiv.org/abs/2406.05365 |
| FaaF | Facts-as-a-Function para checar fatos sem suporte | https://arxiv.org/abs/2403.03888 |
| VERA | Valida/refina contexto RAG antes de gerar | https://arxiv.org/abs/2409.15364 |
| CoV-RAG | Chain-of-verification retrieval+geração | https://arxiv.org/abs/2410.05801 |

## Implicação AETHER

`VerifiedFact` + certificado `violations=0` antes de `memory.write` @200 alinha com verificação **local/determinística**. HITL 255 permanece humano. Não importar LLM externo no T0.
