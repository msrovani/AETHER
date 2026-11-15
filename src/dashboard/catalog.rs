//! Textos de hover: o que cada medidor demonstra no organismo.

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
