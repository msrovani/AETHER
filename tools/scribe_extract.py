#!/usr/bin/env python3
# Escriba — extração de triplas (sujeito, relação, objeto) → facts.jsonl
#
# Papel no playbook (docs/DOACAO_PLAYBOOK.md, Estágio 2 — Extração):
#   corpus ──► escriba (LLM, batch offline) ──► triplas ──► SGDB + shards FHRR
#
# Regras de doutrina:
#   * Fato de LLM entra em QUARENTENA: authority 60–120 (default 120, cap 120).
#     255 = HITL exclusivo; nunca gravar acima do cap.
#   * Temperatura 0 (determinismo); revision do modelo anotada no meta.
#   * JSON estrito: linha malformada é descartada e contabilizada, nunca
#     adivinhada.
#   * NFKC em todo texto; dedup por (sujeito|relação|objeto) normalizado.
#   * O rótulo do átomo é NFC(f"{s}|{r}|{o}") — mesmo contrato do
#     harvest_transplant.py (--jl-only) e do donor_map_embed.py.
#
# Uso:
#   python tools/scribe_extract.py --corpus corpus/pt --out data/rounds/PROD
#   python tools/scribe_extract.py --hf-dataset wikipedia --hf-config 20231101.pt --split test \
#       --limit 50 --out data/rounds/PROD
#   python tools/scribe_extract.py --model mohar07/qwen3-0.6b-kg-triplets ...
#   python tools/scribe_extract.py --corpus docs/ --out R --dry-run   # só plano
#
# Depois do escriba:
#   python tools/donor_map_embed.py --out data/rounds/PROD --dim 768
#   python tools/harvest_transplant.py --jl-only --out data/rounds/PROD \
#       --seed <NOVO> --revision <hf_rev> --no-whiten --donor Qwen/Qwen3-Embedding-0.6B

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import time
import unicodedata
from datetime import date
from pathlib import Path

AUTHORITY_MIN = 60
AUTHORITY_CAP = 120  # doutrina 0254/0258: transplantado ≤ 120; 255 = HITL

SYSTEM_PROMPT = (
    "Você é um escriba de conhecimento. Extraia triplas factuais (sujeito, "
    "relação, objeto) do texto dado. Responda SOMENTE com um array JSON "
    "válido, sem texto antes ou depois, no formato:\n"
    '[{"s": "sujeito", "r": "relacao", "o": "objeto", "c": 0.9}]\n'
    "Regras: entidade canônica em 's' e 'o'; 'r' é uma relação curta em "
    "snake_case minúsculo (ex.: capital, nasceu_em, orbita, usa, minimiza); "
    "'c' é confiança 0..1; extraia apenas fatos explícitos no texto; se o "
    "texto não tiver fatos, responda []."
)


def nfc(s: str) -> str:
    return unicodedata.normalize("NFC", s)


def atom_label(s: str, r: str, o: str) -> str:
    return nfc(f"{s}|{r}|{o}")


def chunk_text(text: str, max_chars: int, overlap: int) -> list[str]:
    """Chunk por frases com overlap, sem cortar palavra ao meio."""
    text = re.sub(r"\s+", " ", text).strip()
    if not text:
        return []
    sentences = re.split(r"(?<=[.!?;])\s+", text)
    chunks: list[str] = []
    cur: list[str] = []
    size = 0
    for sen in sentences:
        sen = sen.strip()
        if not sen:
            continue
        if size + len(sen) > max_chars and cur:
            chunks.append(" ".join(cur))
            # overlap: recupera o final do chunk anterior
            tail = " ".join(cur)[-overlap:] if overlap > 0 else ""
            cur = [tail, sen] if tail else [sen]
            size = len(" ".join(cur))
        else:
            cur.append(sen)
            size += len(sen) + 1
        if size >= max_chars:
            chunks.append(" ".join(cur))
            tail = " ".join(cur)[-overlap:] if overlap > 0 else ""
            cur = [tail] if tail else []
            size = len(" ".join(cur))
    if cur:
        chunks.append(" ".join(cur))
    return [c for c in (x.strip() for x in chunks) if len(c) >= 40]


def load_corpus(args) -> list[dict]:
    """Retorna [{source, lang, text}] em ordem determinística."""
    docs: list[dict] = []
    if args.hf_dataset:
        from datasets import load_dataset

        cfg = args.hf_config if args.hf_config else None
        ds = load_dataset(args.hf_dataset, cfg, split=args.split)
        cols = args.text_cols.split(",")
        for i, row in enumerate(ds):
            parts = [str(row[c]) for c in cols if c in row]
            text = " ".join(parts).strip()
            if len(text) < 40:
                continue
            docs.append(
                {
                    "source": f"hf:{args.hf_dataset}/{args.hf_config or '-'}#{i}",
                    "lang": args.lang,
                    "text": text[: args.max_doc_chars],
                }
            )
            if args.limit and len(docs) >= args.limit:
                break
        return docs

    if not args.corpus:
        raise SystemExit("informe --corpus (arquivo/pasta) ou --hf-dataset")
    root = Path(args.corpus)
    files = sorted(root.rglob("*.txt")) + sorted(root.rglob("*.md")) if root.is_dir() else [root]
    for p in files:
        text = p.read_text(encoding="utf-8", errors="replace")
        if len(text) < 40:
            continue
        docs.append(
            {
                "source": p.as_posix(),
                "lang": args.lang,
                "text": text[: args.max_doc_chars],
            }
        )
        if args.limit and len(docs) >= args.limit:
            break
    return docs


def parse_json_array(raw: str) -> tuple[list[dict], int]:
    """Parse estrito do array JSON; retorna (triplas, n_descartes)."""
    start, end = raw.find("["), raw.rfind("]")
    if start == -1 or end == -1 or end <= start:
        return [], 1
    try:
        arr = json.loads(raw[start : end + 1])
    except json.JSONDecodeError:
        return [], 1
    if not isinstance(arr, list):
        return [], 1
    out, bad = [], 0
    for it in arr:
        if not isinstance(it, dict):
            bad += 1
            continue
        s, r, o = it.get("s"), it.get("r"), it.get("o")
        if not (isinstance(s, str) and isinstance(r, str) and isinstance(o, str)):
            bad += 1
            continue
        s, r, o = s.strip(), r.strip().lower(), o.strip()
        if not s or not r or not o:
            bad += 1
            continue
        try:
            c = float(it.get("c", 0.7))
        except (TypeError, ValueError):
            c = 0.7
        c = min(1.0, max(0.0, c))
        out.append({"s": s, "r": r, "o": o, "c": c})
    return out, bad


def main() -> int:
    ap = argparse.ArgumentParser(description="Escriba: triplas LLM → facts.jsonl (quarentena 60–120)")
    ap.add_argument("--corpus", type=Path, default=None, help="arquivo ou pasta .txt/.md")
    ap.add_argument("--hf-dataset", default=None, help="dataset HF (ex.: oscar-corpus/...")
    ap.add_argument("--hf-config", default=None)
    ap.add_argument("--split", default="test")
    ap.add_argument("--text-cols", default="text", help="colunas de texto, vírgula-separadas")
    ap.add_argument("--limit", type=int, default=0, help="máx. documentos (0 = todos)")
    ap.add_argument("--out", type=Path, default=Path("data/rounds/scribe_prod"))
    ap.add_argument(
        "--model",
        default="Qwen/Qwen3-8B",
        help="escriba HF (Apache-2.0; Qwen3-8B = IFStruct 79.75, melhor JSON estrito <10B)",
    )
    ap.add_argument("--gguf", type=Path, default=None, help="opcional: GGUF q4_k_m via llama-cpp")
    ap.add_argument("--device", default="auto", choices=["auto", "cpu", "cuda"])
    ap.add_argument("--dtype", default="auto", choices=["auto", "fp32", "bf16", "fp16"])
    ap.add_argument("--batch", type=int, default=0, help="0 = auto (CPU 1 / GPU 4)")
    ap.add_argument("--max-chars", type=int, default=1200)
    ap.add_argument("--overlap", type=int, default=200)
    ap.add_argument("--max-doc-chars", type=int, default=20000)
    ap.add_argument("--max-new-tokens", type=int, default=512)
    ap.add_argument("--authority", type=int, default=AUTHORITY_CAP, help="60..120 (cap 120)")
    ap.add_argument("--lang", default="pt")
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--dry-run", action="store_true", help="não carrega modelo; só plano + contagens")
    args = ap.parse_args()

    if not (AUTHORITY_MIN <= args.authority <= AUTHORITY_CAP):
        raise SystemExit(
            f"--authority deve ficar em [{AUTHORITY_MIN}..{AUTHORITY_CAP}] "
            "(quarentena; 255 = HITL exclusivo)"
        )

    docs = load_corpus(args)
    if not docs:
        raise SystemExit("corpus vazio: nada a extrair")

    chunks: list[dict] = []
    for d in docs:
        for ch in chunk_text(d["text"], args.max_chars, args.overlap):
            chunks.append({"source": d["source"], "lang": d["lang"], "text": ch})

    plan = {
        "docs": len(docs),
        "chunks": len(chunks),
        "max_chars": args.max_chars,
        "model": args.model,
        "authority": args.authority,
    }
    if args.dry_run:
        print("[dry-run] plano:", json.dumps(plan, ensure_ascii=False))
        print("[dry-run] nenhum triple gerado (dry-run não finge extração)")
        return 0

    args.out.mkdir(parents=True, exist_ok=True)
    facts_path = args.out / "facts.jsonl"
    meta_path = args.out / "scribe_meta.json"
    t0 = time.time()
    generated = discarded = dropped_dup = 0
    seen: set[str] = set()

    if args.gguf:
        model_id = f"gguf:{args.gguf.name}"
        revision = "gguf-local"
        from llama_cpp import Llama  # type: ignore

        llm = Llama(
            model_path=str(args.gguf), n_ctx=4096, n_gpu_layers=-1, temperature=0.0, seed=args.seed
        )

        def generate(chunks_batch: list[dict]) -> list[str]:
            outs = []
            for ch in chunks_batch:
                prompt = f"{SYSTEM_PROMPT}\n\nTEXTO:\n{ch['text']}"
                resp = llm.create_completion(prompt=prompt, max_tokens=args.max_new_tokens, temperature=0.0)
                outs.append(resp["choices"][0]["text"])
            return outs

        backend = "llama_cpp_gguf"
        n_batch = 1
    else:
        import torch
        from transformers import AutoModelForCausalLM, AutoTokenizer

        torch.manual_seed(args.seed)
        tok = AutoTokenizer.from_pretrained(args.model)
        model_kwargs = {}
        if args.device != "cpu" and torch.cuda.is_available():
            device = "cuda" if args.device == "auto" else args.device
            dtype = {"auto": torch.bfloat16, "bf16": torch.bfloat16, "fp16": torch.float16}[
                args.dtype if args.dtype != "auto" else "bf16"
            ]
            model_kwargs = {"torch_dtype": dtype}
        else:
            device = "cpu"
            model_kwargs = {"torch_dtype": torch.float32}
        model = AutoModelForCausalLM.from_pretrained(args.model, **model_kwargs).to(device)
        model.eval()
        revision = getattr(model.config, "_commit_hash", None) or "unknown-rev"
        model_id = args.model
        backend = "hf_transformers"
        n_batch = args.batch or (4 if device == "cuda" else 1)

        def generate(batch: list[dict]) -> list[str]:
            prompts = [
                tok.apply_chat_template(
                    [
                        {"role": "system", "content": SYSTEM_PROMPT},
                        {"role": "user", "content": ch["text"]},
                    ],
                    tokenize=False,
                    add_generation_prompt=True,
                )
                for ch in batch
            ]
            enc = tok(prompts, return_tensors="pt", padding=True, truncation=True, max_length=3072).to(device)
            import torch as _t

            with _t.no_grad():
                out = model.generate(
                    **enc,
                    max_new_tokens=args.max_new_tokens,
                    do_sample=False,
                    temperature=None,
                    top_p=None,
                    pad_token_id=tok.eos_token_id,
                )
            return [tok.decode(out[i][enc["input_ids"].shape[1]:], skip_special_tokens=True) for i in range(len(batch))]

    with facts_path.open("w", encoding="utf-8") as fh:
        for i in range(0, len(chunks), n_batch):
            batch = chunks[i : i + n_batch]
            try:
                outs = generate(batch)
            except Exception as e:  # noqa: BLE001 — batch inteiro descartado com auditoria
                print(f"[aviso] batch {i} falhou: {e}", file=sys.stderr)
                discarded += len(batch)
                continue
            # Guarda anti-deslocamento (lição MSG 0567): zip silencioso nunca
            # alinha fatos com fonte errada — comprimento divergente é
            # descartado e contabilizado, nunca truncado sem aviso.
            if len(outs) != len(batch):
                print(
                    f"[aviso] batch {i}: gerou {len(outs)} saídas para {len(batch)} chunks — batch descartado",
                    file=sys.stderr,
                )
                discarded += len(batch)
                continue
            for ch, raw in zip(batch, outs):
                triples, bad = parse_json_array(raw)
                discarded += bad
                for t in triples:
                    lab = atom_label(t["s"], t["r"], t["o"])
                    if lab in seen:
                        dropped_dup += 1
                        continue
                    seen.add(lab)
                    generated += 1
                    fh.write(
                        json.dumps(
                            {
                                "subject": nfc(t["s"]),
                                "relation": nfc(t["r"]),
                                "object": nfc(t["o"]),
                                "confidence": round(t["c"], 4),
                                "citation": f"scribe:{model_id}@{revision}|src={ch['source']}",
                                "authority": args.authority,
                                "lang": ch["lang"],
                                "kind": 1,
                                "conflict_seed": False,
                                "wall_ms": int(time.time() * 1000),
                            },
                            ensure_ascii=False,
                        )
                        + "\n"
                    )
            done = min(i + n_batch, len(chunks))
            rate = done / max(time.time() - t0, 1e-6)
            print(f"[escriba] {done}/{len(chunks)} chunks · triplas={generated} · {rate:.2f} chunks/s", flush=True)

    meta = {
        "model": model_id,
        "revision": revision,
        "backend": backend,
        "temperature": 0,
        "seed": args.seed,
        "authority": args.authority,
        "authority_policy": "quarentena_60_120; 255=HITL",
        "docs": len(docs),
        "chunks": len(chunks),
        "triples": generated,
        "discarded": discarded,
        "duplicates_dropped": dropped_dup,
        "batch": n_batch,
        "device": device if not args.gguf else "gguf",
        "elapsed_s": round(time.time() - t0, 1),
        "created": str(date.today()),
    }
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8")
    print("[escriba]", json.dumps(meta, ensure_ascii=False))
    if generated == 0:
        print("[aviso] 0 triplas: revise modelo/prompt/corpus antes de embedar", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
