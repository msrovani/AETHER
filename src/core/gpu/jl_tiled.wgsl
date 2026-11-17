// G1-v3 - GEMM do transplante (JL) com *shared memory*, um bloco por
// (8 palavras de 32 bits) x (32 linhas).
//
// O v2 (1 thread por (palavra, linha), matriz transposta) ficou limitado pelo
// trafego: cada linha rele a matriz inteira -> n_rows x 15,7 MB = 31 GB para
// 2.000 linhas, 530 ms a ~60 GB/s. Aqui o bloco carrega o tile da matriz
// (8 palavras x 384 j) em shared memory e o reusa para 32 linhas: o trafego de
// `mat` cai 32x e sobe para a banda real da placa.
//
// Ordem das somas: j ascendente, continua. O bit e' identico ao caminho CPU
// (os tiles de j sao apenas um fatiamento do mesmo laco, sem reassociacao).

struct P {
    n_rows: u32,
    d: u32,
    bits: u32,
    words: u32, // palavras de 32 bits por linha
};

@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read> mat: array<f32>; // transposta: d x bits
@group(0) @binding(2) var<storage, read> x: array<f32>; // n_rows x d
@group(0) @binding(3) var<storage, read_write> out: array<u32>; // n_rows x words

const TK: u32 = 16u; // j por chunk
const WPB: u32 = 8u; // palavras por bloco (1 por warp)
const RT: u32 = 32u; // linhas por bloco (1 por lane)

var<workgroup> sM: array<f32, 4352>; // [palavra 8][bit 32][jj 16], stride 17
var<workgroup> sX: array<f32, 544>; // [linha 32][jj 16], stride 17

@compute @workgroup_size(256)
fn main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    let tid = lid.x;
    let warp = tid >> 5u; // 0..7 = palavra dentro do bloco
    let lane = tid & 31u; // bit dentro da palavra / linha dentro do bloco

    let word = wid.x * WPB + warp;
    let row = wid.y * RT + lane;

    var acc: array<f32, 32>;
    for (var b: u32 = 0u; b < 32u; b = b + 1u) {
        acc[b] = 0.0;
    }

    let row_ok = row < p.n_rows;
    let word_ok = word < p.words;

    for (var j0: u32 = 0u; j0 < p.d; j0 = j0 + TK) {
        // --- tile da matriz: 8 palavras x 32 bits x 16 j (coalescido por lane) ---
        for (var t: u32 = 0u; t < 16u; t = t + 1u) {
            let combo = warp * 16u + t; // 0..127
            let w = combo >> 4u; // / 16
            let jj = combo & 15u; // % 16
            let j = j0 + jj;
            let wd = wid.x * WPB + w;
            var v: f32 = 0.0;
            if (wd < p.words && j < p.d) {
                v = mat[j * p.bits + wd * 32u + lane];
            }
            sM[w * 544u + lane * 17u + jj] = v;
        }
        // --- tile de linhas: 32 linhas x 16 j ---
        for (var t: u32 = 0u; t < 2u; t = t + 1u) {
            let idx = tid + t * 256u;
            let r = idx >> 4u; // / 16
            let jj = idx & 15u; // % 16
            let j = j0 + jj;
            let rr = wid.y * RT + r;
            var v: f32 = 0.0;
            if (rr < p.n_rows && j < p.d) {
                v = x[rr * p.d + j];
            }
            sX[r * 17u + jj] = v;
        }
        workgroupBarrier();

        // --- 16 j x 32 acumuladores (sM e' broadcast entre as lanes) ---
        if (row_ok && word_ok) {
            let mb0 = warp * 544u;
            for (var jj: u32 = 0u; jj < TK; jj = jj + 1u) {
                let xv = sX[lane * 17u + jj];
                for (var b: u32 = 0u; b < 32u; b = b + 1u) {
                    acc[b] = acc[b] + sM[mb0 + b * 17u + jj] * xv;
                }
            }
        }
        workgroupBarrier();
    }

    if (row_ok && word_ok) {
        var packed: u32 = 0u;
        for (var b: u32 = 0u; b < 32u; b = b + 1u) {
            if (acc[b] >= 0.0) {
                packed = packed | (1u << b);
            }
        }
        out[row * p.words + word] = packed;
    }
}
