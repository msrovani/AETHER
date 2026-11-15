//! Textos de hover / glossário / fichas — fonte única para UI e docs.

/// Tooltip de um medidor global (ACQ, coverage, hot-path…).
pub fn global_tooltip(key: &str) -> &'static str {
    match key {
        "acq" => {
            "ACQ — AETHER Cognitive Quotient.\n\
             Média das 12 dimensões da auditoria (0–100).\n\
             É a nota de inteligência operacional do motor: \
             quanto o organismo prevê, aprende, corrige-se e \
             lembra sem tokens nem backprop.\n\
             Meta de gate: ≥ 99."
        }
        "coverage" => {
            "Cobertura real (real_coverage).\n\
             Fração das dimensões marcadas MEASURED \
             (instrumento ligado ao módulo real, não PROXY).\n\
             1.00 = as 12 facetas cognitivas estão instrumentadas."
        }
        "hot_path" => {
            "Bytes alocados no hot-path.\n\
             Contagem de alocações de heap durante o ciclo cognitivo \
             armado (step_into / query).\n\
             0 = o organismo respira em buffers pré-alocados — \
             previsível em tempo real."
        }
        "dims" => {
            "Número de dimensões reportadas no scorecard.\n\
             O suite completo tem 12 (T01…T12)."
        }
        _ => "Medidor global do scorecard AETHER.",
    }
}

/// Entradas do glossário “O que estou vendo?”.
pub fn glossary_entries() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Inferência",
            "Previsão da trajetória do estado (Koopman/RLS) sem backprop — o organismo antecipa.",
        ),
        (
            "Coerência",
            "Alinhamento multi-escala (RG) + comando estável (EMA). Sem tremor no atuador.",
        ),
        (
            "RAG",
            "Memória holográfica FHRR/BAM: recall associativo sob oclusão/ruído (T12).",
        ),
        (
            "Inteligência / ACQ",
            "Nota operacional 0–100: média T01–T12. Gate de lab ≥ 99.",
        ),
        (
            "cycles/s",
            "Quadros cognitivos completos por segundo (ritmo do organismo).",
        ),
        (
            "eq-t/s",
            "Tokens equivalentes/s ≈ cycles/s × 128 — comparação com mundos tokenizados.",
        ),
        (
            "SOP/s",
            "Operações sinápticas/espectrais por segundo — densidade de trabalho numérico.",
        ),
        (
            "TOP/s",
            "Tera-operações/s aproximadas (SOP/s ÷ 1e12) para comparar regimes.",
        ),
    ]
}

/// Ficha demonstrativa ao clicar em T0x.
#[derive(Clone, Copy, Debug)]
pub struct DimFicha {
    pub id: &'static str,
    pub title: &'static str,
    pub tooltip: &'static str,
    pub interpret: &'static str,
    pub if_low: &'static str,
    pub provenance: &'static str,
}

/// Ficha por ID (T01…T12).
pub fn dimension_ficha(id: &str) -> DimFicha {
    match id {
        "T01" => DimFicha {
            id: "T01",
            title: "Inferência contínua",
            tooltip: dimension_tooltip("T01"),
            interpret: "R²/NRMSE altos = o Koopman prevê bem a dinâmica observada.",
            if_low: "Previsão fraca: o organismo reage tarde ou erra o horizonte.",
            provenance: "cognitive_audit · closed_loop / Koopman-RLS (MEASURED).",
        },
        "T02" => DimFicha {
            id: "T02",
            title: "Confiabilidade sob ruído",
            tooltip: dimension_tooltip("T02"),
            interpret: "10k ciclos hostis sem NaN/alloc = coerência numérica dura.",
            if_low: "Instabilidade numérica: risco de queda em produção suja.",
            provenance: "cognitive_audit · stress noise harness (MEASURED).",
        },
        "T03" => DimFicha {
            id: "T03",
            title: "Aprendizado online",
            tooltip: dimension_tooltip("T03"),
            interpret: "Após ruptura, o modelo reaprende sem gradiente.",
            if_low: "Plasticidade baixa: trava no regime antigo.",
            provenance: "cognitive_audit · regime shift (MEASURED).",
        },
        "T04" => DimFicha {
            id: "T04",
            title: "Autoevolução estrutural",
            tooltip: dimension_tooltip("T04"),
            interpret: "Poda/especiação mantém papéis vivos (Sentinel/Architect/Regulator).",
            if_low: "Topologia colapsa ou silencia — morfologia morta.",
            provenance: "cognitive_audit · morphogenesis/swarm (MEASURED).",
        },
        "T05" => DimFicha {
            id: "T05",
            title: "Autocorreção",
            tooltip: dimension_tooltip("T05"),
            interpret: "CBF Lyapunov + ressync após falha = organismo se estabiliza.",
            if_low: "Não recupera após choque — risco de runaway.",
            provenance: "cognitive_audit · CBF / resync (MEASURED).",
        },
        "T06" => DimFicha {
            id: "T06",
            title: "Coerência multi-escala",
            tooltip: dimension_tooltip("T06"),
            interpret: "RG 64→16→4 + EMA: R alinhado, comando sem chatter.",
            if_low: "Intenção macro e atuador dessincronizam (tremor/chatter).",
            provenance: "cognitive_audit · continuous_action / RG (MEASURED).",
        },
        "T07" => DimFicha {
            id: "T07",
            title: "Busca epistêmica",
            tooltip: dimension_tooltip("T07"),
            interpret: "Explora U sob barreira de Lyapunov — curiosidade dirigida.",
            if_low: "Passividade ou passeio aleatório sem ganho de informação.",
            provenance: "cognitive_audit · active inference (MEASURED).",
        },
        "T08" => DimFicha {
            id: "T08",
            title: "Resposta multimodal",
            tooltip: dimension_tooltip("T08"),
            interpret: "FFT + memória + homeostase + sonhador em cadeia respondem juntos.",
            if_low: "Pipeline composto falha em algum elo — resposta imprecisa.",
            provenance: "cognitive_audit · multimodal chain (MEASURED).",
        },
        "T09" => DimFicha {
            id: "T09",
            title: "Matemática descoberta",
            tooltip: dimension_tooltip("T09"),
            interpret: "SINDy + MAX-CUT bSB recuperam leis esparsas do sinal.",
            if_low: "Não extrai equações úteis — descoberta simbólica fraca.",
            provenance: "cognitive_audit · SINDy / bSB (MEASURED).",
        },
        "T10" => DimFicha {
            id: "T10",
            title: "Resistência a alucinações",
            tooltip: dimension_tooltip("T10"),
            interpret: "Sem evidência (warrant), confiança → 0; não inventa atrator.",
            if_low: "Afirma sem chave — alucinação operacional.",
            provenance: "cognitive_audit · warrant / Langevin U (MEASURED).",
        },
        "T11" => DimFicha {
            id: "T11",
            title: "Imunidade a desvio",
            tooltip: dimension_tooltip("T11"),
            interpret: "Objetivo macro RG + cleanup M5: intenção não deriva.",
            if_low: "Distração/ruído desvia a meta — pensamento drift.",
            provenance: "cognitive_audit · RG objective / M5 (MEASURED).",
        },
        "T12" => DimFicha {
            id: "T12",
            title: "RAG holográfico",
            tooltip: dimension_tooltip("T12"),
            interpret: "Recall@1 + SNR + latência µs: memória associativa em tempo real.",
            if_low: "Esquece sob oclusão/ruído ou recall lento demais.",
            provenance: "cognitive_audit · BAM FHRR store (MEASURED).",
        },
        _ => DimFicha {
            id: "?",
            title: "Dimensão desconhecida",
            tooltip: dimension_tooltip(id),
            interpret: "Sem ficha canônica para este ID.",
            if_low: "—",
            provenance: "—",
        },
    }
}

/// Tooltip por ID de dimensão (T01…T12).
pub fn dimension_tooltip(id: &str) -> &'static str {
    match id {
        "T01" => {
            "T01 — Inferência contínua.\n\
             O operador de Koopman (RLS / Sherman-Morrison) prevê \
             a trajetória do estado sem backprop.\n\
             Mede: R² / NRMSE no horizonte.\n\
             Alto = o organismo antecipa a dinâmica (previsão física)."
        }
        "T02" => {
            "T02 — Confiabilidade sob ruído.\n\
             10k ciclos com perda e SNR hostil: zero NaN, zero alloc.\n\
             Alto = a coerência numérica aguenta o mundo sujo."
        }
        "T03" => {
            "T03 — Aprendizado online.\n\
             Após ruptura de regime, o Koopman reaprende sem gradiente.\n\
             Alto = plasticidade cognitiva (adaptação contínua)."
        }
        "T04" => {
            "T04 — Autoevolução estrutural.\n\
             Poda + especiação (papéis Sentinel/Architect/Regulator).\n\
             Alto = a topologia se reorganiza e permanece viva \
             (não colapsa em silêncio)."
        }
        "T05" => {
            "T05 — Autocorreção.\n\
             CBF de Lyapunov + ressincronização após falha.\n\
             Alto = o organismo se estabiliza sob pertubação extrema."
        }
        "T06" => {
            "T06 — Coerência multi-escala.\n\
             Renormalização 64→16→4 + atuação contínua (EMA).\n\
             Mede alinhamento R e chattering do comando.\n\
             Alto = intenção macro coerente sem tremor no atuador."
        }
        "T07" => {
            "T07 — Busca epistêmica.\n\
             Inferência ativa: explora zonas de incerteza (U) \
             sob barreira de Lyapunov.\n\
             Alto = curiosidade dirigida, não passeio aleatório."
        }
        "T08" => {
            "T08 — Resposta multimodal.\n\
             FFT + memória + homeostase + sonhador em cadeia.\n\
             Alto = pipeline cognitivo composto responde com precisão."
        }
        "T09" => {
            "T09 — Matemática descoberta.\n\
             SINDy recupera equações (dx/dt) + MAX-CUT bSB.\n\
             Alto = o motor extrai leis esparsas do sinal."
        }
        "T10" => {
            "T10 — Resistência a alucinações.\n\
             Gate por evidência (warrant) + U de Langevin.\n\
             Alto = se a chave não existe, a confiança cai a zero \
             (não inventa atrator)."
        }
        "T11" => {
            "T11 — Imunidade a desvio de pensamento.\n\
             Objetivo na camada macro do RG + cleanup M5.\n\
             Alto = a intenção não deriva sob ruído distrator."
        }
        "T12" => {
            "T12 — RAG holográfico.\n\
             Memória FHRR/BAM: recall com oclusão e ruído de fase.\n\
             Mede Recall@1, SNR e latência (µs).\n\
             Alto + rápido = inteligência associativa em tempo real."
        }
        _ => "Dimensão da auditoria cognitiva AETHER.",
    }
}

/// Tooltip de vazão / regime.
pub fn throughput_tooltip(key: &str) -> &'static str {
    match key {
        "p50" => {
            "Latência p50 (mediana) do ciclo cognitivo.\n\
             Metade dos quadros é mais rápida que este valor.\n\
             É a sensação de ‘fluidez’ do organismo em tempo real."
        }
        "p95" | "p99" => {
            "Latência de cauda (p95/p99).\n\
             Quadros lentos sob contenção do SO.\n\
             Importa para garantia de 60 Hz sem drops."
        }
        "p99_9" | "p99.9" => {
            "Latência p99.9 — pior cauda quase extrema.\n\
             Sob carga de CPU sobe muito; com afinidade de núcleo cai.\n\
             Indicador de jitter / preempção, não de ‘estupidez’."
        }
        "cycles" => {
            "Ciclos cognitivos por segundo.\n\
             Quantos quadros completos o organismo fecha por segundo.\n\
             Em Basal ≈ 1/p50 (se a cauda não dominar)."
        }
        "eq_t" | "eq-t" => {
            "eq-t/s — tokens equivalentes por segundo.\n\
             Heurística: cycles/s × 128 (janela típica).\n\
             Traduz a vazão física para uma linguagem comparável \
             a sistemas tokenizados — sem usar tokens de verdade."
        }
        "sop" => {
            "SOP/s — operações sinápticas/espectrais por segundo.\n\
             Volume de trabalho numérico no solver/onda.\n\
             Alto = o hardware está sendo usado de forma densa."
        }
        "tops" => {
            "TOP/s (aprox.) — tera-operações/s normalizadas.\n\
             Visão de pico de intensidade computacional do regime.\n\
             Derivada da vazão SOP; útil para comparar Basal vs Crítico."
        }
        "allocs" => {
            "Alocações no hot-path durante o bench.\n\
             Deve permanecer 0: frames pré-dimensionados."
        }
        "basal" => {
            "Regime Basal — 60 passos de relaxamento/quadro.\n\
             Ritmo ‘em repouso’ do organismo (meta ~60 Hz)."
        }
        "vigilante" => {
            "Regime Vigilante — 120 passos/quadro.\n\
             Mais trabalho termodinâmico por ciclo (alerta)."
        }
        "critico" => {
            "Regime Crítico — 240 passos/quadro.\n\
             Máxima profundidade de relaxamento; menor cycles/s."
        }
        _ => "Métrica de vazão do bench throughput_tps.",
    }
}
