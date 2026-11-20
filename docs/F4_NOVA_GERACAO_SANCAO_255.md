# F4 — sanção para nova geração de dataset (255, 2026-11-20)

**Decisão:** autorizar uma **nova geração** de dataset de avaliação para F4,
com ouro em nível de **arquivo** (ou múltiplas seções válidas por página).
O dataset congelado atual (`data/embed_eval_set.jsonl`, SHA-256 `f8682442…
416bf0`) **permanece intacto** e o veredito REPROVADO dele (MSG 0947) não é
revertido.

**Autoridade:** 255 por delegação explícita do HITL ao gestor OPENCODE
(MSG 0772/0866/0947). Registro no fórum: MSG 1067.

## Por quê

A geração atual falhou o gate `hit@5 ≥ 0.974` com híbrido `0.7465`, e o
diagnóstico (MSG 0926, revisado em 0930) mostrou que 21 de 37 misses acertam o
arquivo e perdem para seção irmã da mesma página — problema de **granularidade
do ouro**, não do modelo. Corrigir isso exige novo dataset; reescrever o atual
seria mover a meta depois de medir.

## Condições obrigatórias (não negociáveis)

1. **Novo arquivo, novo hash.** Criar `data/embed_eval_set_v2.jsonl` (nome
   distinto). Publicar o SHA-256 no fórum **antes** de qualquer medição.
2. **Gate inalterado.** O critério `hit@5 ≥ 0.974` continua; nenhuma redução.
   Uma redefinição de métrica exigiria sanção 255 **separada e explícita**.
3. **Sem tuning depois de medir.** Nenhuma reescrita de ouro, consulta ou
   limiar após o resultado. Se falhar, registra-se REPROVADO para a v2.
4. **Sem chamada externa por default** (Eixo C). Download de modelo, se
   necessário, segue o protocolo de lock de GPU (MSG 0844) e a allowlist.
5. **Proveniência.** Cada rótulo novo entra em quarentena 60–120; só o HITL
   sanciona 255. Nenhum rótulo gerado por modelo vira 255 sozinho.

## Escopo de execução sugerido

O dataset v2 pode ser construído pelo agente de dados (CURSOR) com revisão de
números do AGENT-A (dono do F4); o novo run do MiniLM precisa de lock de GPU no
fórum. Este documento autoriza a **preparação**, não dispara download nem
medição.

## Fora de escopo

Não altera `aether-decision/src/`, `src/agent/cli.rs`, `src/knowledge/sgdb/`,
nem o dataset v1 congelado.
