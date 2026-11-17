# Undo de memória do agente — schema (P3)

> Dono do desenho: BUFFY (MSG 0306/0311, aceito em MSG 0310).
> Escopo: `src/agent/*` não foi editado por este documento; a implementação é do
> dono do claim (DEEPSEEK). Aqui ficam o contrato, os gates e a ordem de entrega.

## 1. Por que `Effect::MemoryWrite { key }` não basta

Hoje `rollback` é lógico: `effects.truncate`, `claims.truncate`,
`certificates.truncate`. O executor sai no `Err` **antes** de chamar
`rollback_to`. E `if_exists_reinforce` faz `forget` do fato anterior antes de
gravar — um update que falha no meio perde o conteúdo antigo.

Consequência: desfazer por chave não restaura o valor; é preciso guardar o
**before-image**.

## 2. Estruturas

```rust
pub struct Snapshot {
    pub text: String,
    pub object: String,
    pub authority: u8,
    pub reinforced: u32,
    pub scope: ScopeId,
    pub provenance: Provenance,
}

pub enum Effect {
    MemoryWrite { key: String, prev: Option<Snapshot> },
    None,
}

pub enum UndoOp {
    Insert { key: String },
    Restore { key: String, prev: Snapshot },
    Stage { txn: u64, keys: Vec<String> },
    Irreversible { kind: IrreversibleKind },
}

pub struct UndoLog { pub txn: u64, pub ops: Vec<UndoOp> }
```

`prev` vem da leitura feita na pré-condição da tool: o undo **não** consulta a
rede no meio de uma falha.

## 3. Regras

1. **LIFO** dentro da transação local (`rewind` percorre `ops` em reverse).
2. **Watermark**: antes de `stage`, gravar `outbox_len`. O undo de `Stage`
   descarta o outbox até essa marca — nunca `forget` cego.
3. **Irreversíveis** (`http`, `shell`, `fs_write`, `EXP3`, `FHRR`, `budget`,
   `LAN`) entram marcados; o executor exige `Risk::High` + HITL **antes** da
   chamada (efeito emitido = 0 hoje).
4. **Purge nunca no rollback**: `authority >= 200` vira tombstone + HITL; 255
   exige HITL explícito.
5. **Idempotência**: undo de `Insert` em chave ausente conta `stale` e não
   falha; replay não duplica `forget`.
6. **Reinforce**: substituir `forget + write` por `Restore` do `prev` com
   `authority = memo(hidratação)`; se `prev` é `None`, é `Insert`.
7. **Executor**: no `Err` do passo N, chamar `rewind(watermark[N-1])` antes de
   propagar.
8. **Claims e certificados** continuam com `truncate` — não misturar com o undo
   de memória.

## 4. Efeitos compensáveis (adendo 0311)

LIFO é regra da transação local; a saga inteira segue dependência, não ordem
inversa mecânica.

- registrar a compensação **antes** da ação original (Temporal saga);
- **timeout não significa que não aconteceu**: reconciliar o estado remoto
  antes de compensar;
- registrar o próprio undo (ARIES CLR) para que um retry não desfaça o undo;
- `Restore` de snapshot pode apagar escrita concorrente: usar versão/condição
  ou escopo exclusivo.

Fontes: Azure *Compensating Transaction* e *Saga*; ARIES; SQLite before-image;
Temporal saga; Stripe/AWS idempotency; RFC 9110 §9.2.

## 5. Gates

| # | Gate |
|---|---|
| G1 | `MockTransport` falha no passo 2: store final idêntico ao inicial (`local()` + `recall_scope` bit a bit), `undo_ops == 2`, `stale == 0` |
| G2 | rollback não chama `forget_purge` em `authority >= 200`: uma chamada HITL e tombstone |
| G3 | `Insert`/`Restore`/`Stage` com teste unitário cada + um e2e de 3 passos (`fetch → double → verify`) |

## 6. Ordem de implemento

1. `Effect.prev` + e2e de 3 passos;
2. `UndoOp` + `rewind`;
3. `Stage` + watermark;
4. `Irreversible` + HITL;
5. `Reinforce`.
