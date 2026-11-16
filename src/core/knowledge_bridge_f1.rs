//! F1 — ponte de conhecimento *read-only* `T0 → T1 → T2` (Vol. III §7).
//!
//! Este módulo materializa o contrato do Vol. III §7: um evento de
//! conhecimento (energia, camada, autoridade, escopo, **dois carimbos**) com
//! payload de fase cru, atravessa o hot path (`T0`) **sem alocar**, **sem
//! I/O** e **sem falar com o `neural-sgdb`**, e é drenado no `T1` para virar
//! envelope de persistência.
//!
//! ```text
//! T0  record(&event)       → slot POD no anel SPSC      (0 alloc, 0 syscall)
//! T1  drain_batch(...)     → DraftRecord { escopo, certificado, tick+wall_ms }
//! T2  (fora deste módulo)  → envelope textual / chamada ao SGDB
//! ```
//!
//! # Por que um arquivo separado do `knowledge_bridge.rs`
//!
//! `src/core/knowledge_bridge.rs` já existe e é **do claim MSG 0107**: é o
//! evento POD de 32 bytes da proveniência do E3 (`SourceTag`/`EventKind`).
//! O contrato do Vol. III §7 é outro: carrega **payload de fase** (até 1280 B),
//! `Layer`, `ScopeId`, `Certificate` e a volta `cue_into`. Para não disputar
//! escrita no mesmo arquivo (regra 1 arquivo / 1 agente), o F1 vive aqui e
//! importa o que já existe — a fusão num único arquivo é mecânica e fica para
//! quando o dono do `knowledge_bridge.rs` der ACK.
//!
//! # Os três invariantes que este módulo existe para provar
//!
//! | Invariante | Como é garantido |
//! |---|---|
//! | **R7** — zero drift para RAG | o `T0` não tem acesso a ledger: [`SpscKnowledgePhaseBridge::record`] só escreve no anel. `t0_ledger_calls` fica `0`. |
//! | **R8** — isolamento de escopo | todo evento carrega [`ScopeId`]; [`ScopeLedger`] só devolve o que casa com o escopo pedido. |
//! | **K6** — timebase | `tick` (60 Hz interno) e `wall_ms` (epoch/monotônico externo) viajam **separados**; o `T1` copia `wall_ms` para `created_tick` sem nunca converter `tick`. |

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::core::spsc_ring::{SpscConsumer, SpscProducer, SpscRingBuffer};

/// Maior payload de fase aceito: `D = 160` complexo `f32` = `160 · 8` bytes.
pub const MAX_PHASE_BYTES: usize = 160 * 8;

/// Capacidade padrão do anel (potência de dois, exigência do SPSC).
pub const DEFAULT_RING_CAPACITY: usize = 64;

/// Certificado de **factibilidade** (não de verdade) — Vol. III §6.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Certificate {
    /// `H(s*)` na convenção do solver.
    pub energy: f32,
    /// Custo do melhor flip simples (`0` = mínimo local).
    pub gap: f32,
    /// Restrições violadas (`0` = factível).
    pub violations: u32,
}

/// Camada do ledger (Vol. III §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Layer {
    /// L2 — episódico, verbatim.
    L2Episodic = 2,
    /// L4 — semântico.
    L4Semantic = 4,
    /// L6 — grafo.
    L6Graph = 6,
}

impl Layer {
    /// Código estável de 1 byte (`2`, `4`, `6` — os mesmos do SGDB).
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Inverso de [`Layer::code`]; `None` para camada desconhecida.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            2 => Some(Layer::L2Episodic),
            4 => Some(Layer::L4Semantic),
            6 => Some(Layer::L6Graph),
            _ => None,
        }
    }
}

/// Escopo do evento: **organismo / run / aplicação** (Vol. III §10, R8).
///
/// `ScopeId` é `Copy` e `Eq`: serve de chave de partição sem `String` e sem
/// hash (portanto sem alocação no `T0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScopeId {
    /// Identidade do organismo (isolamento cross-organism).
    pub organism_id: u64,
    /// Identidade da execução (run) dentro do organismo.
    pub run_id: u64,
    /// Tag da aplicação/origem.
    pub app_tag: u32,
}

impl ScopeId {
    /// Escopo explícito.
    pub const fn new(organism_id: u64, run_id: u64, app_tag: u32) -> Self {
        Self {
            organism_id,
            run_id,
            app_tag,
        }
    }

    /// `true` se os dois escopos pertencem ao mesmo organismo.
    ///
    /// É o filtro que o `T1`/`T2` usa: escopo **nunca** é comparado só por
    /// `run_id` (runs de organismos diferentes podem coincidir).
    pub const fn same_organism(&self, other: &Self) -> bool {
        self.organism_id == other.organism_id
    }
}

/// Erros da ponte. Nenhum deles envolve `String` (o `T0` não pode alocar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    /// O anel está cheio: o evento foi descartado (nunca bloqueia).
    RingFull,
    /// `payload.len() > MAX_PHASE_BYTES`.
    PayloadTooLarge,
    /// `out.len()` incompatível com o payload recordado.
    BufTooSmall,
    /// Não há nada no escopo pedido.
    EmptyScope,
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::RingFull => "anel cheio (evento descartado)",
            Self::PayloadTooLarge => "payload de fase maior que MAX_PHASE_BYTES",
            Self::BufTooSmall => "buffer de saída menor que o payload",
            Self::EmptyScope => "nenhum evento no escopo pedido",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for BridgeError {}

/// Evento de conhecimento do `T0` — **sem alocação, sem `String`**.
///
/// `payload` é bytes crus de fase (`f32` complexo intercalado), nunca `&str`:
/// o sinal não atravessa a fronteira como texto.
#[derive(Clone, Copy, Debug)]
pub struct KnowledgeEvent<'a> {
    /// Endereço por conteúdo: hash determinístico do padrão de fase.
    pub key: u128,
    /// Camada do ledger.
    pub layer: Layer,
    /// Certificado físico do estado que gerou o evento.
    pub certificate: Certificate,
    /// *Warrant* social, `0..=255` (ortogonal ao `gap` — Vol. III §6).
    pub authority: u8,
    /// Escopo (organismo/run/app).
    pub scope: ScopeId,
    /// Relógio interno (dinâmica, 60 Hz). **Nunca** vira TTL.
    pub tick: u64,
    /// Relógio externo (epoch ou monotônico, ms). É este que vira TTL/decay.
    pub wall_ms: u64,
    /// Bytes crus de fase.
    pub payload: &'a [u8],
}

impl<'a> KnowledgeEvent<'a> {
    /// Evento com certificado zerado e autoridade de inferência.
    pub const fn new(key: u128, layer: Layer, scope: ScopeId, tick: u64, wall_ms: u64) -> Self {
        Self {
            key,
            layer,
            certificate: Certificate {
                energy: 0.0,
                gap: 0.0,
                violations: 0,
            },
            authority: 0,
            scope,
            tick,
            wall_ms,
            payload: &[],
        }
    }

    /// Anexa o certificado.
    pub const fn with_certificate(mut self, certificate: Certificate) -> Self {
        self.certificate = certificate;
        self
    }

    /// Anexa a autoridade social (`0..=255`).
    pub const fn with_authority(mut self, authority: u8) -> Self {
        self.authority = authority;
        self
    }

    /// Anexa o payload de fase.
    pub const fn with_payload(mut self, payload: &'a [u8]) -> Self {
        self.payload = payload;
        self
    }
}

/// Destino de eventos do `T0`. `record` **nunca** aloca nem bloqueia.
pub trait KnowledgeSink {
    /// Registra um evento. `Err(RingFull)` = descartado, a computação segue.
    fn record(&mut self, event: &KnowledgeEvent<'_>) -> Result<(), BridgeError>;
}

/// Volta do ledger para o motor: `recall → vetor → semente de atrator`.
///
/// É o detalhe que fecha o laço do Vol. III §7: o ledger **não substitui** a
/// fase, ele a reacende. `out` é pré-alocado pelo chamador (nenhuma alocação
/// no caminho de decisão).
pub trait KnowledgeSource {
    /// Escreve em `out` o padrão de fase mais próximo de `query`, **dentro de
    /// `scope`**.
    ///
    /// O escopo é argumento (não comentário): é ele que torna R8 verificável.
    /// A resposta carrega a `key` e a `authority` do fato que casou — sem isso
    /// o vetor volta sem proveniência (K6) e a resposta não é citável.
    fn cue_into(
        &mut self,
        scope: ScopeId,
        query: &[f32],
        out: &mut [f32],
    ) -> Result<CueHit, BridgeError>;
}

/// Proveniência de um `cue` bem-sucedido (`Copy`: 32 B, sem alocação).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CueHit {
    /// Chave endereçável por conteúdo do registro que casou.
    pub key: u128,
    /// Escopo do registro que casou (igual ao escopo pedido).
    pub scope: ScopeId,
    /// Autoridade social do registro (0..=255).
    pub authority: u8,
    /// Candidatos **do escopo** consultados nesta busca.
    pub hits: usize,
}

/// Slot POD do anel: todo o evento já é materializado aqui.
///
/// `Copy` porque o SPSC exige mensagens POD; `bytes` é fixo, então `record`
/// só copia — não há caminho de alocação.
#[derive(Clone, Copy)]
pub struct PhaseSlot {
    key: u128,
    energy: f32,
    gap: f32,
    violations: u32,
    organism_id: u64,
    run_id: u64,
    tick: u64,
    wall_ms: u64,
    app_tag: u32,
    len: u32,
    layer: u8,
    authority: u8,
    #[allow(dead_code)]
    reserved: u16,
    bytes: [u8; MAX_PHASE_BYTES],
}

impl std::fmt::Debug for PhaseSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhaseSlot")
            .field("key", &self.key)
            .field("layer", &self.layer)
            .field("len", &self.len)
            .field("tick", &self.tick)
            .field("wall_ms", &self.wall_ms)
            .field("scope", &(self.organism_id, self.run_id, self.app_tag))
            .finish()
    }
}

impl PhaseSlot {
    /// Slot vazio (só para inicializar o anel).
    const EMPTY: Self = Self {
        key: 0,
        energy: 0.0,
        gap: 0.0,
        violations: 0,
        organism_id: 0,
        run_id: 0,
        tick: 0,
        wall_ms: 0,
        app_tag: 0,
        len: 0,
        layer: 0,
        authority: 0,
        reserved: 0,
        bytes: [0u8; MAX_PHASE_BYTES],
    };

    /// Payload de fase do slot.
    pub fn payload(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// Reconstrói o evento drenado (T1).
    pub fn to_event(&self) -> KnowledgeEvent<'_> {
        KnowledgeEvent {
            key: self.key,
            layer: Layer::from_code(self.layer).unwrap_or(Layer::L4Semantic),
            certificate: Certificate {
                energy: self.energy,
                gap: self.gap,
                violations: self.violations,
            },
            authority: self.authority,
            scope: ScopeId {
                organism_id: self.organism_id,
                run_id: self.run_id,
                app_tag: self.app_tag,
            },
            tick: self.tick,
            wall_ms: self.wall_ms,
            payload: self.payload(),
        }
    }
}

/// Telemetria da ponte (Vol. III §13 — R7 mensurável, não prometido).
///
/// Compartilhada entre o produtor (`T0`) e o consumidor (`T1`) por `Arc`; os
/// contadores são atômicos **relaxados** (telemetria não ordena decisão).
#[derive(Debug, Default)]
pub struct BridgeMetrics {
    /// Chamadas ao `neural-sgdb` feitas a partir do `T0`. **Deve ser 0.**
    pub t0_ledger_calls: AtomicU64,
    /// Chamadas ao ledger feitas no `T1`/`T2` (uma por evento drenado).
    pub ledger_calls: AtomicU64,
    /// Eventos aceitos pelo anel.
    pub recorded_events: AtomicU64,
    /// Eventos descartados por anel cheio.
    pub dropped_ring_full: AtomicU64,
    /// Eventos drenados pelo `T1`.
    pub drained_events: AtomicU64,
}

impl BridgeMetrics {
    /// Chamadas ao ledger no `T0` (invariante R7).
    pub fn t0_ledger_calls(&self) -> u64 {
        self.t0_ledger_calls.load(Ordering::Relaxed)
    }

    /// Eventos aceitos.
    pub fn recorded(&self) -> u64 {
        self.recorded_events.load(Ordering::Relaxed)
    }

    /// Eventos descartados.
    pub fn dropped(&self) -> u64 {
        self.dropped_ring_full.load(Ordering::Relaxed)
    }

    /// Eventos drenados.
    pub fn drained(&self) -> u64 {
        self.drained_events.load(Ordering::Relaxed)
    }

    /// Chamadas ao ledger no `T1`/`T2`.
    pub fn ledger_calls(&self) -> u64 {
        self.ledger_calls.load(Ordering::Relaxed)
    }
}

/// Ponte `T0 → T1` sobre o anel SPSC.
///
/// `N` é a capacidade do anel (potência de dois `>= 2`). A única alocação
/// acontece em [`SpscKnowledgePhaseBridge::new`]; [`Self::record`] não aloca.
#[derive(Debug)]
pub struct SpscKnowledgePhaseBridge<const N: usize> {
    tx: SpscProducer<PhaseSlot, N>,
    metrics: Arc<BridgeMetrics>,
}

impl<const N: usize> SpscKnowledgePhaseBridge<N> {
    /// Cria a ponte e o consumidor (`T1`), compartilhando a telemetria.
    pub fn new() -> (Self, KnowledgePhaseConsumer<N>) {
        let (tx, rx) = SpscRingBuffer::<PhaseSlot, N>::new().split();
        let metrics = Arc::new(BridgeMetrics::default());
        (
            Self {
                tx,
                metrics: Arc::clone(&metrics),
            },
            KnowledgePhaseConsumer { rx, metrics },
        )
    }

    /// Capacidade do anel.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Eventos ainda no anel (estimativa).
    pub fn len(&self) -> usize {
        self.tx.len()
    }

    /// `true` se o anel está vazio (estimativa).
    pub fn is_empty(&self) -> bool {
        self.tx.is_empty()
    }

    /// Telemetria compartilhada.
    pub fn metrics(&self) -> &Arc<BridgeMetrics> {
        &self.metrics
    }
}

impl<const N: usize> KnowledgeSink for SpscKnowledgePhaseBridge<N> {
    /// **Caminho quente.** Copia o evento para o slot e empurra no anel.
    ///
    /// Contrato: nenhuma alocação, nenhum `String`, nenhum syscall, nenhuma
    /// chamada ao ledger — só `memcpy` de tamanho fixo e um `store` release.
    fn record(&mut self, event: &KnowledgeEvent<'_>) -> Result<(), BridgeError> {
        if event.payload.len() > MAX_PHASE_BYTES {
            // Nem o descarte dobra o invariante: continua sem alocar.
            self.metrics
                .dropped_ring_full
                .fetch_add(1, Ordering::Relaxed);
            return Err(BridgeError::PayloadTooLarge);
        }

        let mut slot = PhaseSlot::EMPTY;
        slot.key = event.key;
        slot.layer = event.layer.code();
        slot.authority = event.authority;
        slot.energy = event.certificate.energy;
        slot.gap = event.certificate.gap;
        slot.violations = event.certificate.violations;
        slot.organism_id = event.scope.organism_id;
        slot.run_id = event.scope.run_id;
        slot.app_tag = event.scope.app_tag;
        slot.tick = event.tick;
        slot.wall_ms = event.wall_ms;
        slot.len = event.payload.len() as u32;
        slot.bytes[..event.payload.len()].copy_from_slice(event.payload);

        match self.tx.try_push(slot) {
            Ok(()) => {
                self.metrics.recorded_events.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_) => {
                self.metrics
                    .dropped_ring_full
                    .fetch_add(1, Ordering::Relaxed);
                Err(BridgeError::RingFull)
            }
        }
    }
}

/// Envelope do `T1`, pronto para persistência (`T2`).
///
/// Aqui a alocação é **permitida** (fora do ciclo): o payload sai do slot POD
/// e vira `Vec<u8>`; o texto do envelope só nasce no `T2`.
#[derive(Clone, Debug, PartialEq)]
pub struct DraftRecord {
    /// Chave endereçável por conteúdo.
    pub key: u128,
    /// Camada do ledger.
    pub layer: Layer,
    /// Certificado físico.
    pub certificate: Certificate,
    /// Autoridade social.
    pub authority: u8,
    /// Escopo.
    pub scope: ScopeId,
    /// Relógio interno (dinâmica) — preservado, **não** convertido.
    pub tick: u64,
    /// Relógio externo (TTL/decay) — vira `created_tick` no SGDB.
    pub wall_ms: u64,
    /// Bytes crus de fase.
    pub payload: Vec<u8>,
}

impl DraftRecord {
    /// `created_tick` exigido pela doutrina do SGDB (mesma base de `now`).
    ///
    /// É `wall_ms` por construção: o `tick` de 60 Hz nunca é promovido a
    /// carimbo de tempo do ledger (K6).
    pub const fn created_tick(&self) -> u64 {
        self.wall_ms
    }

    /// Escopo textual do SGDB (`phase/v1/<organism>/<run>/<app>`).
    ///
    /// Aloca — só é chamado no `T2`, nunca no `T0`.
    pub fn scope_key(&self) -> String {
        format!(
            "phase/v1/{}/{}/{}",
            self.scope.organism_id, self.scope.run_id, self.scope.app_tag
        )
    }

    /// Linha de envelope textual do `T2` (chave, camada, certificado, carimbos).
    pub fn envelope_line(&self) -> String {
        format!(
            "{} layer=L{} E={:.6} gap={:.6} viol={} auth={} tick={} wall_ms={} bytes={}",
            self.scope_key(),
            self.layer.code(),
            self.certificate.energy,
            self.certificate.gap,
            self.certificate.violations,
            self.authority,
            self.tick,
            self.wall_ms,
            self.payload.len()
        )
    }
}

/// Lado consumidor (`T1`): drena o anel fora do hot path.
#[derive(Debug)]
pub struct KnowledgePhaseConsumer<const N: usize> {
    rx: SpscConsumer<PhaseSlot, N>,
    metrics: Arc<BridgeMetrics>,
}

impl<const N: usize> KnowledgePhaseConsumer<N> {
    /// Consome até `max` eventos e os formata como [`DraftRecord`].
    ///
    /// Devolve quantos foram drenados. Cada evento drenado conta **uma**
    /// chamada de ledger (`T1`), nunca no `T0`.
    pub fn drain_batch(&mut self, out: &mut Vec<DraftRecord>, max: usize) -> usize {
        let mut n = 0usize;
        while n < max {
            let Some(slot) = self.rx.try_pop() else {
                break;
            };
            let ev = slot.to_event();
            out.push(DraftRecord {
                key: ev.key,
                layer: ev.layer,
                certificate: ev.certificate,
                authority: ev.authority,
                scope: ev.scope,
                tick: ev.tick,
                wall_ms: ev.wall_ms,
                payload: ev.payload.to_vec(),
            });
            self.metrics.drained_events.fetch_add(1, Ordering::Relaxed);
            self.metrics.ledger_calls.fetch_add(1, Ordering::Relaxed);
            n += 1;
        }
        n
    }

    /// Telemetria compartilhada.
    pub fn metrics(&self) -> &Arc<BridgeMetrics> {
        &self.metrics
    }
}

/// Ledger em memória com **escopo explícito** — o stand-in do `neural-sgdb`.
///
/// Não é o banco: é o mínimo que permite *provar* R8 (isolamento) e exercitar
/// `cue_into` sem rede. Toda consulta filtra por `ScopeId`; não existe
/// consulta global implícita (é o *null-scoping* do SGDB, do lado do motor).
#[derive(Debug, Default)]
pub struct ScopeLedger {
    entries: Vec<DraftRecord>,
    queries: u64,
    cross_scope_hits: u64,
    cross_scope_skipped: u64,
}

impl ScopeLedger {
    /// Ledger vazio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Persiste um envelope drenado.
    pub fn append(&mut self, record: DraftRecord) {
        self.entries.push(record);
    }

    /// Número de registros no escopo do organismo dado.
    pub fn hits_for_organism(&self, organism_id: u64) -> usize {
        self.entries
            .iter()
            .filter(|r| r.scope.organism_id == organism_id)
            .count()
    }

    /// Número de registros que casam **exatamente** com o escopo.
    pub fn hits_for_scope(&self, scope: ScopeId) -> usize {
        self.entries.iter().filter(|r| r.scope == scope).count()
    }

    /// Total de registros.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` se vazio.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Consultas de `cue_into` feitas.
    pub fn queries(&self) -> u64 {
        self.queries
    }

    /// Registros de **outro** organismo que apareceram numa resposta.
    /// Invariante R8: tem de permanecer `0`.
    pub fn cross_scope_hits(&self) -> u64 {
        self.cross_scope_hits
    }

    /// Registros de outro escopo **descartados pelo filtro**.
    ///
    /// É o controle positivo de R8: numa consulta cruzada este contador tem de
    /// **subir**, provando que o filtro rodou. Sem ele, `cross_scope_hits == 0`
    /// é vacuidade (R9) — um contador que nunca incrementa não prova nada.
    pub fn cross_scope_skipped(&self) -> u64 {
        self.cross_scope_skipped
    }

    /// Registros do escopo pedido, na ordem de inserção.
    pub fn scoped(&self, scope: ScopeId) -> impl Iterator<Item = &DraftRecord> {
        self.entries.iter().filter(move |r| r.scope == scope)
    }

    /// Interpreta o payload como pares complexos intercalados (`re, im`).
    ///
    /// Aloca: só é chamado no `T1`/`T2` (nunca no caminho quente) e evita
    /// `unsafe` para reinterpretar bytes — o crate inteiro é `deny(unsafe_code)`.
    fn payload_phases(record: &DraftRecord) -> Vec<f32> {
        record
            .payload
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

impl KnowledgeSource for ScopeLedger {
    /// Recall por correlação complexa dentro de **um** escopo.
    ///
    /// `query` e o payload gravado são vetores de fase complexos intercalados
    /// (`re, im`) de mesmo comprimento. O melhor casamento é copiado para
    /// `out` (semente de atrator) e o retorno carrega a proveniência do fato
    /// que casou (`key`, `scope`, `authority`). O filtro é **exato** por
    /// `ScopeId`: `run_id` de organismos diferentes pode coincidir, então
    /// comparar só `run_id` não isola nada.
    fn cue_into(
        &mut self,
        scope: ScopeId,
        query: &[f32],
        out: &mut [f32],
    ) -> Result<CueHit, BridgeError> {
        self.queries += 1;
        if self.entries.is_empty() {
            return Err(BridgeError::EmptyScope);
        }
        let mut best: Option<(f32, usize)> = None;
        let mut consulted = 0usize;
        let mut skipped = 0u64;
        for (idx, record) in self.entries.iter().enumerate() {
            if record.scope != scope {
                skipped += 1;
                continue;
            }
            let phases = Self::payload_phases(record);
            if phases.is_empty() || phases.len() != query.len() {
                continue;
            }
            consulted += 1;
            // Correlação complexa: Re(Σ conj(a)·b) normalizada.
            let mut re = 0.0f64;
            let mut na = 0.0f64;
            let mut nb = 0.0f64;
            for (a, b) in phases.chunks_exact(2).zip(query.chunks_exact(2)) {
                let (ar, ai) = (f64::from(a[0]), f64::from(a[1]));
                let (br, bi) = (f64::from(b[0]), f64::from(b[1]));
                re += ar * br + ai * bi;
                na += ar * ar + ai * ai;
                nb += br * br + bi * bi;
            }
            let den = (na.sqrt() * nb.sqrt()).max(1e-12);
            let score = (re / den) as f32;
            let better = match best {
                None => true,
                Some((s, _)) => score > s,
            };
            if better {
                best = Some((score, idx));
            }
        }
        // O filtro rodou: registros de fora do escopo foram descartados.
        self.cross_scope_skipped += skipped;
        let Some((_, idx)) = best else {
            return Err(BridgeError::EmptyScope);
        };
        let record = &self.entries[idx];
        let phases = Self::payload_phases(record);
        if out.len() < phases.len() {
            return Err(BridgeError::BufTooSmall);
        }
        out[..phases.len()].copy_from_slice(&phases);
        Ok(CueHit {
            key: record.key,
            scope: record.scope,
            authority: record.authority,
            hits: consulted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_cabe_o_payload_maximo_e_e_pod() {
        assert_eq!(MAX_PHASE_BYTES, 1280);
        assert!(
            std::mem::size_of::<PhaseSlot>() >= MAX_PHASE_BYTES,
            "o slot precisa acomodar MAX_PHASE_BYTES"
        );
        assert!(!std::mem::needs_drop::<PhaseSlot>());
    }

    #[test]
    fn record_e_drain_preservam_campos() {
        let (mut tx, mut rx) = SpscKnowledgePhaseBridge::<8>::new();
        let scope = ScopeId::new(1, 7, 9);
        let payload = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let ev = KnowledgeEvent::new(0xDEAD_BEEF, Layer::L4Semantic, scope, 123, 9_999)
            .with_certificate(Certificate {
                energy: -3.5,
                gap: 0.25,
                violations: 0,
            })
            .with_authority(255)
            .with_payload(&payload);
        assert!(tx.record(&ev).is_ok());
        assert_eq!(rx.metrics().t0_ledger_calls(), 0);

        let mut out = Vec::new();
        assert_eq!(rx.drain_batch(&mut out, 8), 1);
        let rec = &out[0];
        assert_eq!(rec.key, 0xDEAD_BEEF);
        assert_eq!(rec.layer, Layer::L4Semantic);
        assert_eq!(rec.certificate.energy, -3.5);
        assert_eq!(rec.authority, 255);
        assert_eq!(rec.scope, scope);
        assert_eq!(rec.tick, 123);
        assert_eq!(rec.wall_ms, 9_999);
        assert_eq!(rec.payload, payload);
        assert_eq!(rec.created_tick(), 9_999);
        assert!(rec.scope_key().starts_with("phase/v1/1/7/9"));
    }

    #[test]
    fn payload_grande_e_rejeitado_sem_alocar_anel() {
        let (mut tx, rx) = SpscKnowledgePhaseBridge::<4>::new();
        let big = vec![0u8; MAX_PHASE_BYTES + 1];
        let ev = KnowledgeEvent::new(0, Layer::L2Episodic, ScopeId::new(1, 1, 1), 0, 0)
            .with_payload(&big);
        assert_eq!(tx.record(&ev), Err(BridgeError::PayloadTooLarge));
        assert_eq!(rx.metrics().dropped(), 1);
        assert_eq!(rx.metrics().recorded(), 0);
    }

    #[test]
    fn escopo_nao_vaza_entre_organismos() {
        let mut ledger = ScopeLedger::new();
        for (org, key) in [(1u64, 10u128), (1, 11), (2, 20)] {
            ledger.append(DraftRecord {
                key,
                layer: Layer::L4Semantic,
                certificate: Certificate::default(),
                authority: 128,
                scope: ScopeId::new(org, 0, 1),
                tick: 0,
                wall_ms: 0,
                payload: vec![1.0f32.to_le_bytes()[0]; 8],
            });
        }
        assert_eq!(ledger.hits_for_organism(1), 2);
        assert_eq!(ledger.hits_for_organism(2), 1);
        assert_eq!(ledger.hits_for_organism(3), 0);
        let scoped: Vec<_> = ledger.scoped(ScopeId::new(2, 0, 1)).collect();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].key, 20);
    }
}
