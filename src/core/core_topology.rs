//! Topologia de CPU, plano de núcleos e *pinning* de threads.
//!
//! * [`PhysicalCore`]: um núcleo físico = ids lógicos (irmãos SMT) +
//!   classe de eficiência (híbridos P/E: `0` = E-core).
//! * [`CorePlan`]: reflexo no **1º núcleo físico de maior eficiência**
//!   (irmãos SMT **reservados**, ociosos); Koopman-SINDy e SwarmSpeciation nos
//!   núcleos físicos seguintes, de menor eficiência primeiro (mantém os
//!   P-cores livres para o RT).
//! * [`pin_current_thread`]: `core_affinity` (feature `affinity`); sem a
//!   feature ou se o SO recusar ⇒ *no-op* (`false`).
//!
//! ## Detecção
//!
//! | SO | Fonte | Estado |
//! |----|-------|--------|
//! | Windows | `GetLogicalProcessorInformationEx(RelationProcessorCore)` — irmãos SMT e `EfficiencyClass` exatos | **verificado** neste ambiente |
//! | Linux | `/sys/devices/system/cpu/cpuN/topology/thread_siblings_list` | parser testado; **não executado** em Linux aqui |
//! | outros | heurística `logical / physical` ids adjacentes | genérico |
//!
//! Só ids permitidos pela máscara de afinidade do processo
//! (`core_affinity::get_core_ids`) entram no plano. Sem prioridade de tempo
//! real do SO (isso exigiria `SetThreadPriority`/`SCHED_FIFO`).

#![allow(unsafe_code)] // FFI do Windows; análise do buffer é 100 % segura.

/// Núcleo físico: ids lógicos (irmãos SMT) e classe de eficiência.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalCore {
    /// Ids lógicos (ordenados) pertencentes ao núcleo.
    pub logical: Vec<usize>,
    /// Classe de eficiência (maior = mais rápido; híbridos P/E).
    pub efficiency: u8,
}

/// Plano de núcleos lógicos para as três threads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CorePlan {
    /// Núcleo da thread reflexiva (1º físico de maior eficiência).
    pub reflex: Option<usize>,
    /// Núcleo do Koopman-SINDy.
    pub koopman: Option<usize>,
    /// Núcleo do SwarmSpeciation.
    pub swarm: Option<usize>,
    /// Irmãos SMT do núcleo reflexivo, reservados (ociosos).
    pub reserved_siblings: Vec<usize>,
    /// Núcleos lógicos considerados.
    pub logical: usize,
    /// Núcleos físicos considerados.
    pub physical: usize,
}

impl CorePlan {
    /// Plano **puro** a partir dos núcleos físicos.
    pub fn from_cores(cores: &[PhysicalCore]) -> Self {
        let mut cores: Vec<PhysicalCore> = cores
            .iter()
            .filter(|c| !c.logical.is_empty())
            .cloned()
            .collect();
        if cores.is_empty() {
            return Self::default();
        }
        for c in cores.iter_mut() {
            c.logical.sort_unstable();
        }
        cores.sort_by_key(|c| c.logical[0]);
        let logical = cores.iter().map(|c| c.logical.len()).sum();
        let max_eff = cores.iter().map(|c| c.efficiency).max().unwrap_or(0);
        let reflex_pos = cores
            .iter()
            .position(|c| c.efficiency == max_eff)
            .unwrap_or(0);
        let reflex_core = cores.remove(reflex_pos);
        // Secundários: menor eficiência primeiro, depois menor id.
        cores.sort_by_key(|c| (c.efficiency, c.logical[0]));
        let first = cores.first().map(|c| c.logical[0]);
        Self {
            reflex: Some(reflex_core.logical[0]),
            koopman: first,
            swarm: cores.get(1).map(|c| c.logical[0]).or(first),
            reserved_siblings: reflex_core.logical[1..].to_vec(),
            logical,
            physical: cores.len() + 1,
        }
    }

    /// Heurística quando o SO não informa irmãos SMT: `stride = logical /
    /// physical` ids adjacentes por núcleo físico.
    pub fn from_topology(ids: &[usize], physical: usize) -> Self {
        if ids.is_empty() {
            return Self::default();
        }
        let n = ids.len();
        let physical = physical.clamp(1, n);
        let stride = (n / physical).max(1);
        let cores: Vec<PhysicalCore> = ids
            .chunks(stride)
            .map(|c| PhysicalCore {
                logical: c.to_vec(),
                efficiency: 0,
            })
            .collect();
        Self::from_cores(&cores)
    }

    /// Detecta a topologia da máquina (vazio se pinning indisponível).
    pub fn detect() -> Self {
        #[cfg(feature = "affinity")]
        {
            let ids: Vec<usize> = core_affinity::get_core_ids()
                .unwrap_or_default()
                .into_iter()
                .map(|c| c.id)
                .collect();
            if ids.is_empty() {
                return Self::default();
            }
            match os_physical_cores() {
                Some(cores) => {
                    let allowed: Vec<PhysicalCore> = cores
                        .into_iter()
                        .map(|c| PhysicalCore {
                            logical: c.logical.into_iter().filter(|i| ids.contains(i)).collect(),
                            efficiency: c.efficiency,
                        })
                        .collect();
                    Self::from_cores(&allowed)
                }
                None => Self::from_topology(&ids, num_cpus::get_physical()),
            }
        }
        #[cfg(not(feature = "affinity"))]
        {
            Self::default()
        }
    }
}

/// Fixa a thread corrente em `core`; `false` se indisponível (no-op).
pub fn pin_current_thread(core: Option<usize>) -> bool {
    #[cfg(feature = "affinity")]
    {
        match core {
            Some(id) => core_affinity::set_for_current(core_affinity::CoreId { id }),
            None => false,
        }
    }
    #[cfg(not(feature = "affinity"))]
    {
        let _ = core;
        false
    }
}

/// Interpreta `"0-1,8,10-11"` (formato do sysfs do Linux).
pub fn parse_cpu_list(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for part in s.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                    out.extend(a..=b);
                }
            }
            None => {
                if let Ok(v) = part.trim().parse::<usize>() {
                    out.push(v);
                }
            }
        }
    }
    out
}

/// Interpreta o buffer de `GetLogicalProcessorInformationEx(RelationProcessorCore)`.
///
/// Layout (`SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX`, x64): `Relationship`
/// `u32 @0`, `Size u32 @4`, `Flags u8 @8`, `EfficiencyClass u8 @9`,
/// `GroupCount u16 @30`, `GroupMask[] @32` com `{Mask usize, Group u16}` a
/// cada 16 bytes. Id lógico = `Group · 64 + bit`.
pub fn parse_windows_cores(buf: &[u8]) -> Vec<PhysicalCore> {
    let mut cores = Vec::new();
    let mut off = 0usize;
    let rd_u32 = |o: usize| -> Option<u32> {
        buf.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let rd_u16 =
        |o: usize| -> Option<u16> { buf.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])) };
    let rd_mask = |o: usize| -> Option<u64> {
        buf.get(o..o + 8)
            .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    };
    while let (Some(rel), Some(size)) = (rd_u32(off), rd_u32(off + 4)) {
        if size < 8 {
            break;
        }
        if rel == 0 {
            let eff = buf.get(off + 9).copied().unwrap_or(0);
            let groups = rd_u16(off + 30).unwrap_or(0) as usize;
            let mut logical = Vec::new();
            for g in 0..groups {
                let base = off + 32 + 16 * g;
                if let (Some(mask), Some(group)) = (rd_mask(base), rd_u16(base + 8)) {
                    for bit in 0..64usize {
                        if mask >> bit & 1 == 1 {
                            logical.push(group as usize * 64 + bit);
                        }
                    }
                }
            }
            if !logical.is_empty() {
                cores.push(PhysicalCore {
                    logical,
                    efficiency: eff,
                });
            }
        }
        off += size as usize;
    }
    cores
}

#[cfg(all(feature = "affinity", windows))]
fn os_physical_cores() -> Option<Vec<PhysicalCore>> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalProcessorInformationEx(
            relationship: u32,
            buffer: *mut u8,
            returned_length: *mut u32,
        ) -> i32;
    }
    let mut len = 0u32;
    // SAFETY: consulta de tamanho (buffer nulo, `len` válido); o retorno de
    // erro `ERROR_INSUFFICIENT_BUFFER` é esperado.
    unsafe { GetLogicalProcessorInformationEx(0, std::ptr::null_mut(), &mut len) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` tem exatamente `len` bytes e `len` aponta para um `u32`.
    let ok = unsafe { GetLogicalProcessorInformationEx(0, buf.as_mut_ptr(), &mut len) };
    if ok == 0 {
        return None;
    }
    let cores = parse_windows_cores(&buf[..len as usize]);
    (!cores.is_empty()).then_some(cores)
}

#[cfg(all(feature = "affinity", target_os = "linux"))]
fn os_physical_cores() -> Option<Vec<PhysicalCore>> {
    let n = std::thread::available_parallelism().ok()?.get();
    let mut cores: Vec<PhysicalCore> = Vec::new();
    for id in 0..n {
        let path = format!("/sys/devices/system/cpu/cpu{id}/topology/thread_siblings_list");
        let list = parse_cpu_list(&std::fs::read_to_string(path).ok()?);
        if list.first() == Some(&id) {
            cores.push(PhysicalCore {
                logical: list,
                efficiency: 0,
            });
        }
    }
    (!cores.is_empty()).then_some(cores)
}

#[cfg(all(feature = "affinity", not(windows), not(target_os = "linux")))]
fn os_physical_cores() -> Option<Vec<PhysicalCore>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(logical: &[usize], efficiency: u8) -> PhysicalCore {
        PhysicalCore {
            logical: logical.to_vec(),
            efficiency,
        }
    }

    #[test]
    fn hibrido_reflexo_no_primeiro_p_core_e_secundarios_em_e_cores() {
        // 2 P-cores (HT) + 2 E-cores (como um i7 híbrido reduzido).
        let cores = [
            core(&[0, 1], 1),
            core(&[2, 3], 1),
            core(&[4], 0),
            core(&[5], 0),
        ];
        let p = CorePlan::from_cores(&cores);
        assert_eq!(p.reflex, Some(0));
        assert_eq!(p.reserved_siblings, vec![1]);
        assert_eq!(p.koopman, Some(4));
        assert_eq!(p.swarm, Some(5));
        assert_eq!((p.logical, p.physical), (6, 4));
    }

    #[test]
    fn heuristica_smt_e_poucos_nucleos() {
        let ids: Vec<usize> = (0..16).collect();
        let p = CorePlan::from_topology(&ids, 8);
        assert_eq!(
            (p.reflex, p.koopman, p.swarm, p.reserved_siblings),
            (Some(0), Some(2), Some(4), vec![1])
        );
        let p = CorePlan::from_topology(&[0, 1], 2);
        assert_eq!((p.reflex, p.koopman, p.swarm), (Some(0), Some(1), Some(1)));
        let p = CorePlan::from_topology(&[0], 1);
        assert_eq!((p.reflex, p.koopman, p.swarm), (Some(0), None, None));
        assert_eq!(CorePlan::from_topology(&[], 4), CorePlan::default());
    }

    #[test]
    fn parser_do_sysfs() {
        assert_eq!(parse_cpu_list("0-1,8,10-11\n"), vec![0, 1, 8, 10, 11]);
        assert_eq!(parse_cpu_list("3"), vec![3]);
        assert!(parse_cpu_list("").is_empty());
    }

    #[test]
    fn parser_do_buffer_do_windows() {
        // Dois núcleos: P (eff 1, ids 0,1) e E (eff 0, id 2); entradas de 48 B.
        let mut buf = Vec::new();
        for (eff, mask) in [(1u8, 0b011u64), (0u8, 0b100u64)] {
            let mut e = vec![0u8; 48];
            e[0..4].copy_from_slice(&0u32.to_le_bytes()); // RelationProcessorCore
            e[4..8].copy_from_slice(&48u32.to_le_bytes()); // Size
            e[8] = 1; // Flags
            e[9] = eff;
            e[30..32].copy_from_slice(&1u16.to_le_bytes()); // GroupCount
            e[32..40].copy_from_slice(&mask.to_le_bytes());
            buf.extend_from_slice(&e);
        }
        let cores = parse_windows_cores(&buf);
        assert_eq!(cores, vec![core(&[0, 1], 1), core(&[2], 0)]);
    }
}
