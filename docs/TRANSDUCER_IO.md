# TRANSDUCER_IO — a fronteira física única (Módulo 8a)

O AETHER não começa em texto: começa em um **campo contínuo**. `transducer_io` é a
única porta de entrada do organismo — qualquer produto (A, B ou C) pluga aqui.

```text
fonte física ──► TransducerIo::sample_into ──► FieldNormalizer ──► SurpriseTracker ──► F(t)
 (kernel/CAN/CSI/áudio)        u(t) ∈ [-1,1]^64        z-score/tanh        erro preditivo
```

## 1. Contrato

```rust
pub const FIELD_DIM: usize = 64;
pub const PHASE_CHANNELS: usize = 4;

pub trait TransducerIo {
    fn dim(&self) -> usize;
    fn nominal_hz(&self) -> f32;
    fn sample_into(&mut self, out: &mut [f32]) -> Result<()>;
}
```

Regras:

* `out.len() == dim()`, senão `AetherError::Shape`.
* valores em `[-1, 1]`, sempre finitos.
* `sample_into` **não aloca** após a construção (invariante testado com allocador contador).
* nenhuma `String`/token atravessa esta borda.

## 2. Componentes

| Tipo | Papel | Alocação |
|---|---|---|
| `SyntheticTransducer` | fonte determinística (baseline harmônico + rajada agendável) | só no `new` |
| `FieldNormalizer` | z-score online por canal + `tanh` → `(-1,1)` | só no `new` |
| `SurpriseTracker` | erro de predição auto-calibrado → `F ∈ [0,1)` | só no `new` |
| `PhaseMapper` | campo de 64 canais → 4 fases em `[0,2π)` (contrato M3/M5) | zero |

`SurpriseTracker` compara o erro atual com a **razão de regime** aprendida nos
primeiros 30 quadros (`ratio_base`). Consequência: `F ≈ 0` na assinatura
saudável de qualquer dispositivo, independente da escala dos sensores; só o
excesso de erro que o preditor não absorve eleva `F`.

## 3. Backends e produtos

| Produto | Backend | Sinal | Estado |
|---|---|---|---|
| **C** — escudo de sistema | telemetria do SO (thread jitter, CPU, disco, rede) | `m8` daemon | próximo slice |
| **A** — copiloto veicular | CAN/OBD-II (ELM327, SocketCAN) | vibração, RPM, frenagem | contrato pronto |
| **B** — casa cognitiva | CSI Wi-Fi / microfone de baixa energia | presença, movimento | contrato pronto |
| — | `SyntheticTransducer` | baseline + rajada | **implementado** |

Todos produzem o mesmo `u(t)`; o motor (`WaveInterferenceBlock` → `Surprise` →
`ThermodynamicDreamer` → `MorphogeneticField`) não sabe qual produto está plugado.

## 4. Demonstração

```powershell
.\scripts\cargo.ps1 run --release --example transducer_probe --features m8
```

12 s a 60 Hz com rajada de 2 s em `t=4s`:

```text
! 4.00s  0.851  ----------------------------------------
! 4.10s  0.742  ....................................
     ...
pico da rajada: F=0.851
detectado em 0.0 ms (janela fisica de 60 Hz)
```

## 5. Invariantes (`tests/transducer_invariants.rs`)

1. `dim`/`hz` corretos; buffer de tamanho errado → `Shape`.
2. mesma semente ⇒ mesmo campo bit-a-bit.
3. baseline estável (`F` médio < 0.35); rajada detectada (`pico > 0.6` e
   `pico > baseline + 0.3`); retorno ao baseline após a rajada.
4. fases sempre finitas e em `[0, 2π)`.
5. 600 quadros no hot path com **zero alocações**.

```powershell
.\scripts\cargo.ps1 test --features m8 --test transducer_invariants
```

## 6. Próximo slice (C)

1. coletor Windows no **binário** `aether_daemon` (`m8`): `GetSystemTimes`,
   `GlobalMemoryStatusEx`, contadores PDH de disco/rede e jitter de acordar;
2. `SystemTelemetry: TransducerIo` mapeando cada métrica para canais do campo;
3. teste de aceitação: rajada real de escrita em disco + CPU eleva `F` acima do
   baseline e o pico é localizado no processo/volume;
4. multicast UDP de 40 bytes (`PhasePacket`) para acoplar fases entre máquinas.
