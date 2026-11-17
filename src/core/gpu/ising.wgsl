// G3 - sweep de instancias Ising em lote, v2: **um warp por instancia**.
//
// O kernel v1 (uma thread por instancia, arrays por thread em memoria local)
// ficou limitado a ~17 GB/s de leitura de J: 48 passos x 16 MB = 768 MB para
// 10^4 instancias de n=20 -> 44 ms, so 2,8x. O custo escalava exatamente com
// n^2, ou seja, era trafego/latencia, nao FLOP.
//
// Aqui cada lane e' um sitio i (n <= 32); x_i, y_i, s_i e g_i ficam em
// registradores e o estado compartilhado (x, s) vai para memoria de workgroup,
// sincronizada por barreira. J e' lido **coalescido**: para j fixo, as lanes
// leem jm[jbase + j*n + lane], que e' J[j][lane] = J[lane][j] por simetria.
//
// Contrato: `J` simetrico e com diagonal zero (o mesmo do solver auditado).
// Com `J` assimetrico o resultado difere do CPU (que usa a linha i) - e' o
// preco da leitura coalescida, documentado em docs/GPU_PLAN.md.

struct P {
    n: u32,
    steps: u32,
    k: u32,
    pad: u32,
};

@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read> jm: array<f32>;
@group(0) @binding(2) var<storage, read> hm: array<f32>;
@group(0) @binding(3) var<storage, read> init: array<f32>;
@group(0) @binding(4) var<storage, read> c0m: array<f32>;
@group(0) @binding(5) var<storage, read_write> oute: array<f32>;
@group(0) @binding(6) var<storage, read_write> outs: array<i32>;

// Estado compartilhado por instancia (um workgroup = um warp = uma instancia).
var<workgroup> sx: array<f32, 32>;      // x por sitio
var<workgroup> ss: array<f32, 32>;      // s = sign(x) por sitio
var<workgroup> sb: array<f32, 32>;      // elite da trajetoria
var<workgroup> sw: array<f32, 32>;      // resultado da fuga
var<workgroup> sacc: array<f32, 32>;    // reducao numerica
var<workgroup> sidx: array<u32, 32>;    // reducao argmin

const BIG: f32 = 1e30;

// H(s) = -0.5 * s' J s - h' s, com reducao em arvore no warp. Deixa `sacc[0]`
// com a energia e sincroniza o warp antes de devolver.
fn energy_sum(n: u32, jbase: u32, hi: f32, lane: u32, on: bool) -> f32 {
    var acc: f32 = 0.0;
    if (on) {
        for (var j: u32 = 0u; j < n; j = j + 1u) {
            acc = acc + jm[jbase + j * n + lane] * ss[j];
        }
    }
    var contrib: f32 = 0.0;
    if (on) {
        contrib = -0.5 * ss[lane] * acc - hi * ss[lane];
    }
    sacc[lane] = contrib;
    workgroupBarrier();
    for (var stride: u32 = 16u; stride > 0u; stride = stride >> 1u) {
        if (lane < stride) {
            sacc[lane] = sacc[lane] + sacc[lane + stride];
        }
        workgroupBarrier();
    }
    let e = sacc[0];
    workgroupBarrier();
    return e;
}

// Descida gulosa determinista sobre `ss` (mesma regra do `descend` do CPU:
// menor Delta H_i = 2 s_i (g_i + h_i), empate -> menor indice, ate nao reduzir
// ou esgotar 8n flips). Devolve H(s) final.
fn descend(n: u32, jbase: u32, hi: f32, lane: u32, on: bool) -> f32 {
    var g: f32 = 0.0;
    if (on) {
        for (var j: u32 = 0u; j < n; j = j + 1u) {
            g = g + jm[jbase + j * n + lane] * ss[j];
        }
    }
    for (var step: u32 = 0u; step < 8u * n; step = step + 1u) {
        var delta: f32 = BIG;
        if (on) {
            delta = 2.0 * ss[lane] * (g + hi);
        }
        sacc[lane] = delta;
        sidx[lane] = lane;
        workgroupBarrier();
        for (var stride: u32 = 16u; stride > 0u; stride = stride >> 1u) {
            if (lane < stride) {
                let a = sacc[lane];
                let b = sacc[lane + stride];
                // estritamente menor: empate mantem o menor indice
                if (b < a) {
                    sacc[lane] = b;
                    sidx[lane] = sidx[lane + stride];
                }
            }
            workgroupBarrier();
        }
        let pick = sidx[0];
        let pick_delta = sacc[0];
        workgroupBarrier();
        if (pick_delta >= -1e-12) {
            break;
        }
        let ds = -2.0 * ss[pick];
        if (lane == pick) {
            ss[pick] = -ss[pick];
        }
        if (on) {
            g = g + jm[jbase + pick * n + lane] * ds;
        }
        workgroupBarrier();
    }
    return energy_sum(n, jbase, hi, lane, on);
}

@compute @workgroup_size(32)
fn main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let inst = wid.x;
    let lane = lid.x;
    if (inst >= p.k) {
        return;
    }
    let n = p.n;
    let on = lane < n;
    let jbase = inst * n * n;
    let hbase = inst * n;
    let ibase = inst * n * 2u;

    var x: f32 = 0.0;
    var y: f32 = 0.0;
    var hi: f32 = 0.0;
    if (on) {
        x = init[ibase + 2u * lane];
        y = init[ibase + 2u * lane + 1u];
        hi = hm[hbase + lane];
    }
    var s: f32 = select(-1.0, 1.0, x >= 0.0);
    sx[lane] = x;
    ss[lane] = s;
    sb[lane] = s;
    sw[lane] = s;
    workgroupBarrier();

    var e0 = energy_sum(n, jbase, hi, lane, on);
    var e_best = e0;

    let a0: f32 = 1.0;
    let dt: f32 = 0.05;
    let c0 = c0m[inst];
    let steps_f = f32(p.steps);

    for (var kk: u32 = 0u; kk < p.steps; kk = kk + 1u) {
        let a = a0 * f32(kk + 1u) / steps_f;
        let drift = a0 - a;
        var hx: f32 = 0.0;
        if (on) {
            for (var j: u32 = 0u; j < n; j = j + 1u) {
                hx = hx + jm[jbase + j * n + lane] * sx[j];
            }
            y = y + dt * (-drift * x + c0 * (hx + hi));
            let xn = x + dt * a0 * y;
            if (xn > 1.0) {
                x = 1.0;
                y = 0.0;
            } else if (xn < -1.0) {
                x = -1.0;
                y = 0.0;
            } else {
                x = xn;
            }
            s = select(-1.0, 1.0, x >= 0.0);
            sx[lane] = x;
            ss[lane] = s;
        }
        workgroupBarrier();
        let e = energy_sum(n, jbase, hi, lane, on);
        if (e < e_best) {
            e_best = e;
            sb[lane] = s;
        }
        workgroupBarrier();
    }

    // ---------- polimento: descend(elite) ----------
    ss[lane] = sb[lane];
    workgroupBarrier();
    var e_work = descend(n, jbase, hi, lane, on);
    sw[lane] = ss[lane];

    // ---------- fuga determinista (so se o polimento nao bateu E(s_0)) ----------
    if (e_work >= e0) {
        sb[lane] = ss[lane];
        var best_e = e_work;
        for (var i: u32 = 0u; i < n; i = i + 1u) {
            var st = sb[lane];
            if (lane == i) {
                st = -st;
            }
            ss[lane] = st;
            workgroupBarrier();
            let e_try = descend(n, jbase, hi, lane, on);
            if (e_try < best_e) {
                best_e = e_try;
                sw[lane] = ss[lane];
            }
            workgroupBarrier();
        }
        workgroupBarrier();
    }

    // ---------- saida ----------
    ss[lane] = sw[lane];
    workgroupBarrier();
    let e_out = energy_sum(n, jbase, hi, lane, on);
    if (lane == 0u) {
        oute[inst] = e_out;
    }
    if (on) {
        outs[inst * n + lane] = i32(ss[lane]);
    }
}
