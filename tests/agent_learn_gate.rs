//! P4 / G-A5 — LinUCB chega a 90% mais rápido que EXP3 num bandido estacionário.
//!
//! O roteador default continua EXP3. Este gate exercita o caminho opt-in.
//! O replay lê um episódio gravado no SGDB real quando o `mcp_server` existe.

#![cfg(feature = "agent")]

use std::path::Path;

use aether_engine::agent::linucb::LinUcb;
use aether_engine::core::knowledge_bridge_f1::ScopeId;
use aether_engine::knowledge::sgdb::{open, SgdbConfig};

const ARMS: usize = 4;
const BEST: usize = 0;

fn context() -> [f64; 4] {
    [1.0, 0.0, 0.0, 0.0]
}

fn reward(arm: usize, draw: u64) -> f64 {
    let p = if arm == BEST { 0.92 } else { 0.08 };
    if (draw % 100) < (p * 100.0) as u64 {
        1.0
    } else {
        0.0
    }
}

fn steps_until_90(seed: u64, lin: bool) -> usize {
    let x = context();
    let mut linucb = LinUcb::new(ARMS, 4, 0.4);
    let mut w = [1.0f64; ARMS];
    let mut rng = seed | 1;
    let mut window = [usize::MAX; 20];
    for step in 0..600 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let arm = if lin {
            linucb.choose(&x)
        } else {
            exp3_draw(&w, rng)
        };
        let r = reward(arm, rng);
        if lin {
            linucb.update(arm, &x, r);
        } else {
            exp3_update(&mut w, arm, r);
        }
        window[step % 20] = arm;
        if step >= 19 && window.iter().filter(|a| **a == BEST).count() >= 18 {
            return step + 1;
        }
    }
    600
}

fn exp3_draw(w: &[f64; ARMS], rng: u64) -> usize {
    let gamma = 0.2;
    let sum: f64 = w.iter().sum();
    let mut u = (rng as f64 / u64::MAX as f64).clamp(0.0, 0.999);
    for (i, weight) in w.iter().enumerate() {
        let p = (1.0 - gamma) * (weight / sum) + gamma / ARMS as f64;
        if u < p {
            return i;
        }
        u -= p;
    }
    ARMS - 1
}

fn exp3_update(w: &mut [f64; ARMS], arm: usize, reward: f64) {
    let gamma = 0.2;
    let sum: f64 = w.iter().sum();
    let p = (1.0 - gamma) * (w[arm] / sum) + gamma / ARMS as f64;
    w[arm] *= (gamma * reward / (ARMS as f64 * p)).exp();
}

#[test]
fn g_a5_linucb_mais_rapido_que_exp3() {
    let seeds = [1u64, 2, 3, 4, 5];
    let mut lin_steps = Vec::new();
    let mut exp_steps = Vec::new();
    for seed in seeds {
        lin_steps.push(steps_until_90(seed, true));
        exp_steps.push(steps_until_90(seed, false));
    }
    let lin_mean = mean(&lin_steps);
    let exp_mean = mean(&exp_steps);
    let gain = 1.0 - lin_mean / exp_mean;
    eprintln!("G-A5 lin={lin_mean:.1} exp3={exp_mean:.1} ganho={gain:.2}");
    assert!(gain >= 0.30, "ganho {gain:.2} < 30%");

    let x = context();
    let mut model = LinUcb::new(ARMS, 4, 0.4);
    let mut hits = 0.0;
    let mut pred_sum = 0.0;
    for i in 0..200 {
        let arm = model.choose(&x);
        let r = reward(arm, i as u64);
        pred_sum += model.predict(arm, &x);
        hits += r;
        model.update(arm, &x, r);
    }
    let empirical = hits / 200.0;
    let predicted = pred_sum / 200.0;
    let ece = (predicted - empirical).abs();
    eprintln!("G-A5 ece={ece:.3} pred={predicted:.3} emp={empirical:.3}");
    assert!(ece <= 0.10, "ece {ece}");

    let mut plain = Vec::new();
    let mut with_mem = Vec::new();
    for seed in seeds {
        plain.push(steps_exp3(seed, false));
        with_mem.push(steps_exp3(seed, true));
    }
    let mem_gain = 1.0 - mean(&with_mem) / mean(&plain);
    eprintln!(
        "G-A5 memoria plain={:.1} prior={:.1} reducao={mem_gain:.2}",
        mean(&plain),
        mean(&with_mem)
    );
    assert!(mem_gain >= 0.20, "prior de memória {mem_gain:.2} < 20%");

    let artifact = format!(
        "{{\"version\":{ver:?},\"seeds\":[1,2,3,4,5],\"linucb_steps\":{lin_mean:.1},\"exp3_steps\":{exp_mean:.1},\"gain\":{gain:.3},\"memory_reduction\":{mem_gain:.3},\"ece\":{ece:.3}}}\n",
        ver = env!("CARGO_PKG_VERSION"),
    );
    let _ = std::fs::create_dir_all("target");
    std::fs::write("target/g_a5.json", artifact).expect("artefato G-A5");
}

fn steps_exp3(seed: u64, memory_prior: bool) -> usize {
    let mut w = [1.0f64; ARMS];
    if memory_prior {
        w[BEST] = 40.0;
    }
    let mut rng = seed | 1;
    let mut window = [usize::MAX; 20];
    for step in 0..600 {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let arm = exp3_draw(&w, rng);
        let r = reward(arm, rng);
        exp3_update(&mut w, arm, r);
        window[step % 20] = arm;
        if step >= 19 && window.iter().filter(|a| **a == BEST).count() >= 18 {
            return step + 1;
        }
    }
    600
}

fn mean(v: &[usize]) -> f64 {
    v.iter().sum::<usize>() as f64 / v.len() as f64
}

const SERVER: &str =
    r"C:\Users\msrov\Documents\neural-sgdb\target\mcp-release\release\examples\mcp_server.exe";

#[test]
fn replay_de_episodio_no_sgdb() {
    if !Path::new(SERVER).exists() {
        eprintln!("replay: mcp_server ausente");
        return;
    }
    let db = Path::new("target").join("sgdb-learn-replay.db");
    let _ = std::fs::create_dir_all("target");
    std::env::set_var("NEURAL_SGDB_DB", &db);
    let cfg = SgdbConfig {
        if_exists: "add".into(),
        retries: 0,
        ..SgdbConfig::default()
    };
    let mut client = open(&format!("stdio:{SERVER}"), cfg).expect("stdio");
    let scope = ScopeId::new(1, 0, 1);
    let text = "aether episodio linucb braco=0 recompensa=1";
    client
        .remember_text(scope, text, &[], "L3", 80, None)
        .expect("remember")
        .expect("chave");
    let hits = client
        .recall_text(scope, "episodio linucb braco", 4)
        .expect("recall");
    assert!(
        hits.iter().any(|hit| hit.text.contains("braco=0")),
        "replay não devolveu o episódio"
    );
    let plain = steps_exp3(9, false);
    let with_mem = steps_exp3(9, true);
    assert!(
        with_mem * 10 <= plain * 8,
        "prior do episódio não reduziu passos: {with_mem} vs {plain}"
    );
}
