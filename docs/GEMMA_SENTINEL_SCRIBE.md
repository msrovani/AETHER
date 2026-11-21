# Gemma: sentinela ∧ escriba (economia de meios)

> **Canônico.** Um artefato Gemma · dois plugues · um download.  
> Plano Cursor: *Gemma sentinela escriba*. User final = HW de produto (`host_probe`).

## Papéis

| Papel | Tomada | Contrato |
|---|---|---|
| **Sentinela** | T-DEC (reader Vega / AND) | `decide` → OOD; **não** gera prosa |
| **Escriba** | T-NL-OUT (`GemmaScribePort`) | claims → frase → juíz P2 → `CertifiedUtterance` |

Opt-in escriba: `AETHER_SCRIBE=gemma`. Default chat = **template** até smoke.  
Live neural: `AETHER_SCRIBE_LIVE=1` (+ pesos no path do lock). Sem live = prosa ancorada (só claims), mesmo path.

## Path compartilhado

`.vega/profiles/active/engine.lock.json`:

- `sentinel.path` / `sentinel.id`
- `scribe_enabled: true`
- `scribe_path` (= path/id da sentinela)

Cutover/revert T-DEC atualiza **ambos** os papéis. Candidate nunca no bus.

## Runtime (automático)

`host_probe` / VRAM livre vs pico AND+scribe:

- folga → weights residentes  
- apertado → single-flight (decide XOR generate)  
- sem scribe / OOM → fallback template  

## Doutrina

- Auth prosa ≤120; 255 só HITL  
- Sem claim → Empty / “não sei”  
- `act` + prosa scribe → P6 AND  
- Nunca `vegaml.decide` no escriba  

## Código

- `src/agent/gemma_scribe.rs` — port + política  
- `src/agent/nl_out.rs` — `certify_utterance(&mut dyn LanguagePort)`  
- `src/agent/bus.rs` — backend no trio  
- Catálogo: `roles: ["sentinel","scribe"]`  

Guia usuário: [USER_GUIDE.md](USER_GUIDE.md) §7c.
