//! F1 — ponte de conhecimento *read-only* (produtor SPSC, zero alocação).
//!
//! O E3 (transplantação geométrica) precisa **carimbar a origem** de cada átomo
//! e **registrar** o que o verificador estrutural concluiu — sem bloquear o
//! ciclo e sem alocar. Este módulo é o mínimo que fecha esse requisito:
//!
//! * [`SourceTag`] — a origem de um átomo (`Transplanted`, `Observed`,
//!   `Inferred`, `Axiomatic`, `Human`). É o vocabulário de 1 byte que o E3
//!   anexa a toda resposta; [`SourceTag::to_provenance`] o converte para o
//!   [`crate::core::certificate::Provenance`] do certificado (E3b).
//! * [`KnowledgeEvent`] — mensagem POD de 32 bytes (sem `String`, sem `Box`).
//! * [`KnowledgeSink`] — trait de saída. O caminho quente só chama
//!   `try_emit`, que nunca aloca e nunca bloqueia.
//! * [`SpscKnowledgeBridge`] — implementação sobre
//!   [`crate::core::spsc_ring::SpscRingBuffer`]. Quando a fila enche, o evento
//!   é **contado como descartado** e a computação segue: perder telemetria é
//!   preferível a atrasar o sinal.
//! * [`NullSink`] — sink inerte (verificador desligado).
//!
//! # Disciplina de zero-alloc (Vol. III §7.1)
//!
//! Nenhum método de escrita aloca: o anel é alocado **uma vez** em
//! [`SpscKnowledgeBridge::new`]; o evento é `Copy` e não tem `Drop`.
//!
//! # O que este módulo **não** é
//!
//! Não fala com o `neural-sgdb`, não consolida (`F2`) e não decide escopo
//! (`F3`). Ele só entrega eventos a quem consome o anel — o encaixe de
//! consolidação continua fora do hot path.

use crate::core::spsc_ring::{SpscProducer, SpscRingBuffer};

/// Origem de um conhecimento transplantado/observado/inferido.
///
/// `code()` é estável (logs binários e `KnowledgeEvent::source`); os rótulos
/// são só para exibição e **não** atravessam o sinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceTag {
    /// Medido por um transdutor físico.
    Observed,
    /// Derivado por regra/energia a partir de outra memória.
    Inferred,
    /// Importado de um espaço semântico externo (projeção JL).
    Transplanted,
    /// Definido por contrato/axioma do sistema.
    Axiomatic,
    /// Aprovado por humano (HITL).
    Human,
}

impl SourceTag {
    /// Código estável de 1 byte.
    pub const fn code(self) -> u8 {
        match self {
            SourceTag::Observed => 0,
            SourceTag::Inferred => 1,
            SourceTag::Transplanted => 2,
            SourceTag::Axiomatic => 3,
            SourceTag::Human => 4,
        }
    }

    /// Rótulo estável (exibição apenas).
    pub const fn label(self) -> &'static str {
        match self {
            SourceTag::Observed => "observed",
            SourceTag::Inferred => "inferred",
            SourceTag::Transplanted => "transplanted",
            SourceTag::Axiomatic => "axiomatic",
            SourceTag::Human => "human",
        }
    }

    /// Inverso de [`SourceTag::code`]; `None` para código desconhecido.
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(SourceTag::Observed),
            1 => Some(SourceTag::Inferred),
            2 => Some(SourceTag::Transplanted),
            3 => Some(SourceTag::Axiomatic),
            4 => Some(SourceTag::Human),
            _ => None,
        }
    }

    /// `true` se a tag é uma fonte de evidência (serve de lastro para `Inferred`).
    pub const fn is_evidence(self) -> bool {
        matches!(self, SourceTag::Observed | SourceTag::Transplanted)
    }

    /// Converte para o vocabulário do certificado ([`Provenance`]).
    pub const fn to_provenance(self) -> crate::core::certificate::Provenance {
        use crate::core::certificate::Provenance;
        match self {
            SourceTag::Observed => Provenance::Observed,
            SourceTag::Inferred => Provenance::Inferred,
            SourceTag::Transplanted => Provenance::Transplanted,
            SourceTag::Axiomatic => Provenance::Axiomatic,
            SourceTag::Human => Provenance::Human,
        }
    }
}

/// Tipo de evento registrado no bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    /// Um fato foi gravado/transplantado.
    Fact = 0,
    /// O verificador estrutural avaliou um fato.
    Verification = 1,
    /// Uma consulta foi respondida (com proveniência).
    Retrieval = 2,
    /// Uma consulta foi abstida (margem insuficiente / fora do domínio).
    Abstention = 3,
}

impl EventKind {
    /// Código estável de 1 byte.
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// Veredito de 1 byte anexado ao evento.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Verdict {
    /// Coerente / aceito.
    Ok = 0,
    /// Inconsistência estrutural.
    Violation = 1,
    /// Abstenção (geometria ambígua ou fora do domínio).
    Abstain = 2,
}

impl Verdict {
    /// Código estável de 1 byte.
    pub const fn code(self) -> u8 {
        self as u8
    }
}

/// Mensagem POD de 32 bytes — nenhum ponteiro, nenhum `Drop`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct KnowledgeEvent {
    /// Número de sequência do produtor.
    pub seq: u32,
    /// [`EventKind`] (`u8`).
    pub kind: u8,
    /// [`SourceTag`] (`u8`).
    pub source: u8,
    /// [`Verdict`] (`u8`).
    pub verdict: u8,
    /// Reservado (mantém o alinhamento e o tamanho em 32 B).
    pub flags: u8,
    /// Identificador da entidade (0 = não se aplica).
    pub entity: u32,
    /// Identificador da relação.
    pub relation: u32,
    /// Identificador do valor.
    pub value: u32,
    /// Similaridade/margem em Q30 (`i32::MIN` = não se aplica).
    pub score_q30: i32,
    /// Timestamp monotônico do chamador (ns).
    pub stamp_ns: u64,
}

impl KnowledgeEvent {
    /// Evento com `stamp_ns = 0` e `score_q30` indefinido.
    pub const fn new(
        seq: u32,
        kind: EventKind,
        source: SourceTag,
        verdict: Verdict,
        entity: u32,
        relation: u32,
        value: u32,
    ) -> Self {
        Self {
            seq,
            kind: kind.code(),
            source: source.code(),
            verdict: verdict.code(),
            flags: 0,
            entity,
            relation,
            value,
            score_q30: i32::MIN,
            stamp_ns: 0,
        }
    }

    /// Preenche a margem em Q30 (`f32` em `[-1, 1]`).
    pub fn with_score(mut self, score: f32) -> Self {
        let s = if score.is_finite() { score } else { 0.0 };
        self.score_q30 = (f64::from(s) * f64::from(1u32 << 30) as f64) as i32;
        self
    }

    /// Preenche o timestamp do chamador.
    pub fn with_stamp(mut self, stamp_ns: u64) -> Self {
        self.stamp_ns = stamp_ns;
        self
    }

    /// Margem de volta para `f32`.
    pub fn score(&self) -> f32 {
        if self.score_q30 == i32::MIN {
            f32::NAN
        } else {
            (f64::from(self.score_q30) / f64::from(1u32 << 30) as f64) as f32
        }
    }
}

/// Saída de eventos de conhecimento. `try_emit` **nunca** aloca nem bloqueia.
pub trait KnowledgeSink {
    /// Enfileira um evento. `false` = descartado (fila cheia ou sink inerte).
    fn try_emit(&mut self, event: KnowledgeEvent) -> bool;

    /// Eventos aceitos desde a criação.
    fn emitted(&self) -> u64;

    /// Eventos descartados desde a criação.
    fn dropped(&self) -> u64;

    /// `true` quando o sink realmente entrega eventos.
    fn is_active(&self) -> bool {
        true
    }
}

/// Sink inerte (verificador rodando sem bridge ligado).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullSink;

impl KnowledgeSink for NullSink {
    fn try_emit(&mut self, _event: KnowledgeEvent) -> bool {
        false
    }
    fn emitted(&self) -> u64 {
        0
    }
    fn dropped(&self) -> u64 {
        0
    }
    fn is_active(&self) -> bool {
        false
    }
}

/// Ponte SPSC: o produtor escreve, outra thread (consolidação) lê.
///
/// `N` é a capacidade do anel e precisa ser potência de dois `>= 2`.
#[derive(Debug)]
pub struct SpscKnowledgeBridge<const N: usize> {
    tx: SpscProducer<KnowledgeEvent, N>,
    seq: u32,
    emitted: u64,
    dropped: u64,
}

impl<const N: usize> SpscKnowledgeBridge<N> {
    /// Cria a ponte e devolve também o consumidor.
    pub fn new() -> (Self, crate::core::spsc_ring::SpscConsumer<KnowledgeEvent, N>) {
        let (tx, rx) = SpscRingBuffer::<KnowledgeEvent, N>::new().split();
        (
            Self {
                tx,
                seq: 0,
                emitted: 0,
                dropped: 0,
            },
            rx,
        )
    }

    /// Capacidade do anel.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Eventos ainda na fila (estimativa).
    pub fn len(&self) -> usize {
        self.tx.len()
    }

    /// `true` se a fila está vazia (estimativa).
    pub fn is_empty(&self) -> bool {
        self.tx.is_empty()
    }
}

impl<const N: usize> KnowledgeSink for SpscKnowledgeBridge<N> {
    fn try_emit(&mut self, mut event: KnowledgeEvent) -> bool {
        event.seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        match self.tx.try_push(event) {
            Ok(()) => {
                self.emitted += 1;
                true
            }
            Err(_) => {
                self.dropped += 1;
                false
            }
        }
    }
    fn emitted(&self) -> u64 {
        self.emitted
    }
    fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evento_tem_32_bytes_e_sem_drop() {
        assert_eq!(std::mem::size_of::<KnowledgeEvent>(), 32);
        assert_eq!(std::mem::align_of::<KnowledgeEvent>(), 8);
        assert!(!std::mem::needs_drop::<KnowledgeEvent>());
    }

    #[test]
    fn tags_mapeiam_para_provenance() {
        for tag in [
            SourceTag::Observed,
            SourceTag::Inferred,
            SourceTag::Transplanted,
            SourceTag::Axiomatic,
            SourceTag::Human,
        ] {
            assert_eq!(SourceTag::from_code(tag.code()), Some(tag));
            assert_eq!(tag.to_provenance().code(), tag.code());
        }
        assert_eq!(SourceTag::from_code(9), None);
    }

    #[test]
    fn bridge_emite_em_ordem_e_conta_descarte() {
        let (mut bridge, mut rx) = SpscKnowledgeBridge::<4>::new();
        let ev = KnowledgeEvent::new(0, EventKind::Fact, SourceTag::Transplanted, Verdict::Ok, 1, 2, 3);
        for _ in 0..4 {
            assert!(bridge.try_emit(ev));
        }
        // quinta falha sem panicar: só conta o descarte.
        assert!(!bridge.try_emit(ev));
        assert_eq!(bridge.emitted(), 4);
        assert_eq!(bridge.dropped(), 1);
        for k in 0..4u32 {
            let got = rx.try_pop().expect("evento");
            assert_eq!(got.seq, k);
            assert_eq!(got.kind, EventKind::Fact.code());
            assert_eq!(got.source, SourceTag::Transplanted.code());
        }
        assert!(rx.try_pop().is_none());
    }

    #[test]
    fn score_q30_ida_e_volta() {
        let ev = KnowledgeEvent::new(0, EventKind::Retrieval, SourceTag::Observed, Verdict::Ok, 0, 0, 0)
            .with_score(0.25);
        assert!((ev.score() - 0.25).abs() < 1e-9);
        let nan = KnowledgeEvent::new(1, EventKind::Retrieval, SourceTag::Observed, Verdict::Ok, 0, 0, 0);
        assert!(nan.score().is_nan());
    }
}
