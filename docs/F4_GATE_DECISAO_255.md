# F4 — decisão do gate (255, 2026-11-20)

**Decisão:** Opção B. Manter o critério de aceite `hit@5 ≥ 0.974` e declarar
**F4 REPROVADO nesta geração**. Não redefinir o gate para nível de arquivo e não
reescrever o dataset congelado.

**Autoridade:** 255 por delegação do HITL ao gestor OPENCODE (MSG 0866/0945).
Registro no fórum: MSG 0946. Fato nsgdb: `aether/agent`, autoridade 255.

## Evidência (medida, não inferida)

| Item | Valor |
|---|---|
| Dataset congelado | `data/embed_eval_set.jsonl`, SHA-256 `f8682442…cce576`, 722 docs / 142 consultas |
| BM25 (baseline lexical, CURSOR MSG 0875) | `hit@5 = 0.7042` |
| allMiniLM-L6v2 (AGENT-A MSG 0926, 1ª medição) | embed `0.4085`, híbrido `0.5915` — FAIL |
| paraphrase-multilingual-MiniLM-L12-v2 384d (2ª medição) | embed `0.5141`, híbrido `0.7465` — FAIL |
| Gate do plano | `hit@5 ≥ 0.974` — não atingido |
| Diagnóstico dominante | 21 de 37 misses acertam o arquivo e perdem para seção irmã da mesma página |
| hit@5 por arquivo (não rederivado; hipótese) | `0.8873` vs `0.7394` por seção |

Fontes: MSG 0926 (AGENT-A), MSG 0930 (review independente do CURSOR: SHA,
BM25, embed e híbrido conferem), MSG 0938 (opções A/B documentadas).

## Fundamento

1. **Não mudar o critério depois de medir.** A ressalva D3 (MSG 0772) exigia
   corpus congelado antes da medição e hash publicado, sem tuning posterior.
   Redefinir o gate para nível de arquivo após ver o resultado seria mover a
   meta — exatamente o que a doutrina do projeto proíbe ("não baixar teto").
2. **O híbrido é melhor que o baseline, mas isso não é o gate.** `0.7465 >
   0.7042` é um ganho relativo real e deve ser registrado como tal; não
   transforma `0.7465` em `0.974`.
3. **A causa é de construção de ouro, não de modelo.** A maioria dos erros é
   arquivo correto/seção irmã. Isso é uma propriedade do dataset, não uma
   deficiência do embedder — e a correção pertence a uma próxima geração de
   dataset, com nova sanção, não a um rebaixamento agora.
4. **Integridade do congelamento.** Nenhum arquivo de `data/embed_eval_set.jsonl`
   foi alterado; o SHA publicado permanece `f8682442…cce576`.

## Consequências

- F4 encerrado como **REPROVADO nesta geração**; não é gate de liberação do
  híbrido semântico em produção.
- O resultado `híbrido 0.7465 > lexical 0.7042` fica registrado como evidência
  de que a direção híbrida é promissora, sem autorizar uso como fato sancionado.
- T3 (regressão hit@k com F4) permanece na fila; o baseline BM25 de 0.7042
  continua sendo o piso lexicial comparável.

## Próximo passo (exige nova sanção, não executado aqui)

Gerar uma **nova** geração de dataset com ouro em nível de arquivo (ou múltiplas
seções válidas por página), com hash próprio congelado **antes** de medir, e
submeter novo gate ao HITL. Nada neste documento autoriza reescrever o dataset
atual ou redefinir o limiar de 0.974.
