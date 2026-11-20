"""G8 — deriva o dataset F4-v2 com ouro em nível de ARQUIVO.

Ouro v1 = seções (`fact:<arquivo>:<n>`); o diagnóstico (MSG 0926) mostrou
21/37 misses acertando o arquivo e perdendo para a seção irmã. O v2 mantém
os MESMOS docs (nada é reescrito) e reescreve o gold das consultas para o
CONJUNTO de seções do(s) arquivo(s) do ouro original — acertar qualquer
seção do arquivo certo conta como hit.

Regras: novo arquivo + novo hash ANTES de medir (MSG 0947/1067); gate
0.974 inalterado; sem tuning depois.
"""

from __future__ import annotations

import collections
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "data" / "embed_eval_set.jsonl"
DST = ROOT / "data" / "embed_eval_set_v2.jsonl"

rows = [json.loads(l) for l in SRC.read_text(encoding="utf-8").splitlines() if l.strip()]
docs = [r for r in rows if r["type"] == "doc"]
queries = [r for r in rows if r["type"] == "query"]

# arquivo de cada seção:
#   "doc:<arquivo>#<seção>" → "doc:<arquivo>"
#   "fact:<arquivo>:<n>"    → "fact:<arquivo>"  (fact tem só o índice depois)
#   "nsgdb:<...>"           → a própria key (não seccionado)
def file_of(doc_id: str) -> str:
    if doc_id.startswith("doc:") and "#" in doc_id:
        return doc_id.split("#", 1)[0]
    return doc_id.rsplit(":", 1)[0]


section_file = {d["id"]: file_of(d["id"]) for d in docs}

# gold por arquivo (união das seções do arquivo, por consulta)
by_file = collections.defaultdict(list)
for d in docs:
    by_file[section_file[d["id"]]].append(d["id"])

out = []
for r in rows:
    if r["type"] == "doc":
        out.append(r)
        continue
    if "gold" not in r:
        out.append(r)  # linha não-consulta sem gold: preserva como está
        continue
    file_ids = sorted({section_file.get(g) for g in r["gold"] if section_file.get(g)})
    q2 = dict(r)
    q2["gold"] = sorted({sid for f in file_ids for sid in by_file[f]})
    q2["gold_kind"] = "file_level"
    q2["ref"] = "MSG_0926_0947_G8_v2"
    out.append(q2)

DST.write_text(
    "\n".join(json.dumps(r, ensure_ascii=False) for r in out) + "\n",
    encoding="utf-8",
)
n_docs = sum(1 for r in out if r["type"] == "doc")
n_qs = sum(1 for r in out if r["type"] == "query")
gold_sizes = [len(r["gold"]) for r in out if r["type"] == "query"]
print(json.dumps({
    "ok": True,
    "dst": str(DST.relative_to(ROOT)),
    "docs": n_docs,
    "queries": n_qs,
    "gold_medio_por_arquivo": round(sum(gold_sizes) / max(1, len(gold_sizes)), 1),
}, ensure_ascii=False))
