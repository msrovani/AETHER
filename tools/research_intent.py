#!/usr/bin/env python3
"""Classifica pedido antes da pesquisa externa.

Contrato HITL: ou acha evidência utilizável, ou **nega conhecimento**.
Não fica no limbo pesquisa-vazia (fontes=0).

Ações:
  research — fato enciclopédico / mundo aberto → cascata DDG/SearX/Wiki
  memory   — assunto interno AETHER → só memória local (sem web)
  deny     — lixo / controle / geração / sem chance de evidência → negar
"""

from __future__ import annotations

import re
import unicodedata

# Controles e cortesia — não são perguntas de conhecimento.
_CONTROL = re.compile(
    r"^(obrigado|obrigada|valeu|ok|okay|ping|pong|sair|exit|quit|fim|bye|"
    r"tchau|repita|repeat|oi|ola|olá|hey)\b",
    re.I,
)

# Pedidos generativos / chat livre — AETHER não é LLM de prosa.
_GENERATIVE = re.compile(
    r"\b(escreva|escreve|compose|poema|poesia|historia|história|conto|"
    r"piada|música|musica|letra de|roleplay|finja que)\b|"
    r"fale (em portugues )?sobre o tempo|como (está|esta) o tempo",
    re.I,
)

# Injeção / social-engineering — negar (escalar fica a cargo do port).
_INJECTION = re.compile(
    r"ignore (as )?regras|diga a senha|reveal (the )?password|"
    r"jailbreak|DAN mode|system prompt",
    re.I,
)

# Vocabulário do produto — evidência só na memória local, não na web genérica.
_INTERNAL = re.compile(
    r"\b(aether|aether-agent|neural-sgdb|nsgdb|hub dashboard|gate f4|"
    r"f4\b|exp3|auditoria cognitiva|usabilidade do hub|api externa do aether|"
    r"bloco fourier|false act|abstain quando|escopo vaza|llm gera|"
    r"memoria semantica|hitl|autoridade 255|quem decide 255|"
    r"contrato de usabilidade|status do gate)\b",
    re.I,
)

# Pistas de fato mundano / enciclopédico — vale cascata web (HITL item 3).
_FACTISH = re.compile(
    r"\b("
    r"capital|quem escreveu|autor(?:a|es)?\s+d[eo]|quem (?:criou|inventou|pintou|comp[oô]s)|"
    r"dist[aâ]ncia|quantos|quantas|quando (?:nasceu|foi|ocorreu|aconteceu)|"
    r"onde (?:fica|fica a|fica o|nasceu|fica situad)|"
    r"o que [eé] (?:o|a|um|uma)|quem [eé]|"
    r"algoritmo|defin[ií][cç][aã]o|significa|"
    r"popula[cç][aã]o|presidente|primeiro[- ]ministro|rei d[eo]|rainha d[eo]|inventor|"
    r"em que (?:ano|país|pais|cidade)|qual (?:a|o|é|e) |"
    r"massa d[eo]|velocidade d[eo]|altura d[eo]|tamanho d[eo]"
    r")\b|"
    r"\b("
    r"brasil|fran[cç]a|jap[aã]o|portugal|alemanha|espanha|it[aá]lia|"
    r"china|india|índia|eua|estados unidos|reino unido|"
    r"terra|lua|marte|sol|saturno|j[uú]piter|"
    r"paris|londres|berlim|roma|lisboa|t[oó]quio|tokyo"
    r")\b",
    re.I,
)

_JUNK_TOKEN = re.compile(r"^[a-z]{1,4}$", re.I)


def _fold(s: str) -> str:
    nk = unicodedata.normalize("NFKD", s)
    return "".join(c for c in nk if not unicodedata.combining(c)).casefold()


def _alnum_tokens(text: str) -> list[str]:
    return re.findall(r"[a-z0-9à-ú]{2,}", text.casefold())


def classify_request(text: str) -> dict:
    """Devolve {action, reason, line}.

    action ∈ research|memory|deny
    """
    raw = (text or "").strip()
    if not raw:
        return {
            "action": "deny",
            "reason": "vazio",
            "line": "desconhecimento: pedido vazio — sem evidência",
        }

    # Só pontuação / gritos.
    if not re.search(r"[a-zA-ZÀ-ú0-9]", raw):
        return {
            "action": "deny",
            "reason": "lixo_simbolos",
            "line": "desconhecimento: entrada sem conteúdo — não há o que saber",
        }

    toks = _alnum_tokens(raw)
    folded = _fold(raw)

    if _CONTROL.search(raw) and len(toks) <= 3:
        return {
            "action": "deny",
            "reason": "controle",
            "line": "desconhecimento: comando/cortesia — sem fato a recuperar",
        }

    if _INJECTION.search(raw):
        return {
            "action": "deny",
            "reason": "injecao",
            "line": "desconhecimento: pedido de bypass/segredo — recusado",
        }

    if _GENERATIVE.search(raw):
        return {
            "action": "deny",
            "reason": "generativo",
            "line": "desconhecimento: pedido generativo — AETHER não inventa prosa",
        }

    # Lixo tipo asdf qwer / tokens curtos aleatórios sem verbo factual.
    if len(toks) >= 2 and all(_JUNK_TOKEN.match(t) for t in toks) and not _FACTISH.search(raw):
        return {
            "action": "deny",
            "reason": "lixo_tokens",
            "line": "desconhecimento: texto ilegível — sem evidência possível",
        }

    if len(toks) < 2 and not _FACTISH.search(raw):
        return {
            "action": "deny",
            "reason": "curto_demais",
            "line": "desconhecimento: pedido incompleto — sem evidência",
        }

    if _INTERNAL.search(raw) or "aether" in folded or "neural-sgdb" in folded:
        # Interno: web genérica é improdutiva; memória local é a solução.
        # Linha NÃO começa com desconhecimento — o CLI decide cite vs deny.
        return {
            "action": "memory",
            "reason": "interno_projeto",
            "line": "memória: assunto interno — web fechada",
        }

    if _FACTISH.search(raw):
        return {
            "action": "research",
            "reason": "fato_mundano",
            "line": "pesquisa: candidata",
        }

    # Default conservador: negar em vez de gastar rede no escuro.
    return {
        "action": "deny",
        "reason": "sem_pista_factual",
        "line": "desconhecimento: sem pista factual — não pesquiso nem invento",
    }


if __name__ == "__main__":
    import json
    import sys

    q = " ".join(sys.argv[1:]) or "teste"
    print(json.dumps(classify_request(q), ensure_ascii=False, indent=2))
