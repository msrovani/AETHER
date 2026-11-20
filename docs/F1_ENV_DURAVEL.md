# F1 — Contrato do ambiente do engine (durabilidade)

Data: 2026-11-20 · Autor: OPENCODE (sessão F1) · Referências: fórum MSG 0748,
0752, 0792, 0793, 0794 · Plano: `docs/VEGA_F3_PLANO_CONSOLIDADO.md` (F1, Eixos A/D/E)

## O problema (medido, não opinativo)

O plano da F1 exige:

> *torch 2.14.1+cu126 + python pinados no lockfile (upgrade = rebuild + re-probe)*

A realidade medida na máquina:

| fato | medição |
|---|---|
| onde o motor vivia | `target/vega/` — **4.697 MB** (venv + 2 checkpoints) |
| quem apaga | `cargo clean` do ciclo automático — **já aconteceu** (MSG 0748: `target_py_embed FOI APAGADO por cargo_clean`) |
| lockfile de ambiente | **não existia** |
| pins medidos | python 3.14.10 · vegaml 0.16.5 · torch 2.14.1+cu126 · triton-windows 3.8.0 · fla 0.5.0 · transformers 5.23.0 |
| checkpoints | `vega-u-gemma-1b` e `vega-u-gemma-1b-prose`, 63,8 MB cada |

Ou seja: o motor do AETHER — o único que produz as decisões — morava num
diretório que o build apaga. Um `cargo clean` trocava o engine por nada, e o
chat passaria a abster "honestamente" para sempre, sem que ninguém percebesse
a diferença entre "engine fora" e "engine quebrada".

## A segunda armadilha: o embed não enxerga o venv

Medido com `aether-decision/examples/f1_env_probe.rs`:

```
executable    = ...\target\debug\examples\f1_env_probe.exe
prefix        = C:\Users\msrov\AppData\Local\Programs\Python\Python314
base_prefix   = C:\Users\msrov\AppData\Local\Programs\Python\Python314
sys.path      = ...\Python314\Lib\site-packages      <- o BASE, não o venv
import vegaml (sem ajuste)  = FALHOU: ModuleNotFoundError: No module named 'vegaml'
insert site-packages        = OK
import vegaml (com ajuste)  = OK  versao=Some("0.16.5")
torch=Some("2.14.1+cu126") cuda=Some(true)
```

O PyO3 (`auto-initialize`) embute o **Python base** da máquina. O `PYO3_PYTHON`
é um pin de **build**: ele decide qual interpreter é linkado/embutido, mas o
`sys.path` do processo não herda o `site-packages` do venv. Resultado: o crate
compila verde e mesmo assim `import vegaml` morre.

Duas saídas, ambas válidas, e elas se combinam:

1. **pin de runtime** — `PYTHONPATH=<venv>/Lib/site-packages` no lançamento
   (é o que o BUFFY mediu no MSG 0793);
2. **pin de config** — `EngineConfig.python_site_packages` + o bootstrap
   injeta o `site-packages` no `sys.path` **antes** do `import vegaml`
   (o campo já existe no crate; o consumo é do binding PyO3).

## O contrato (o que passa a valer)

1. **O motor mora fora de `target/`.** Layout novo em `.vega/` (gitignored):
   - `.vega/venv/` — o interpretador e os pins;
   - `.vega/models/vega-u-gemma-1b/` e `.vega/models/vega-u-gemma-1b-prose/`.
2. **`data/vega_env.lock.json` é o lockfile** que o plano exigia: os 47
   pacotes medidos, com pin exato, mais o índice CUDA do torch.
3. **`tools/vega_env_setup.ps1` materializa e verifica** o lockfile:
   cria o venv, instala os pins, copia os checkpoints e confere
   python/vegaml/torch/cuda. Sai != 0 se algum pin divergir.
4. **`PYTHONPATH`/`VEGA_SITE_PACKAGES` acompanha o motor.** O embed não
   adivinha o venv.
5. **Pin não sobe.** Mudou versão → rebuild + re-probe n=30 antes/depois
   (Eixo D). O upgrade silencioso é proibido.

### Comandos

```powershell
# materializar o ambiente durável (custo: download do torch ~2,5 GB)
powershell -NoProfile -ExecutionPolicy Bypass -File tools/vega_env_setup.ps1

# só conferir os pins do ambiente já existente
powershell -NoProfile -ExecutionPolicy Bypass -File tools/vega_env_setup.ps1 `
  -VerifyOnly -Venv target/vega

# build com a feature que embute o interpreter
$env:PYO3_PYTHON = "$PWD\.vega\venv\Scripts\python.exe"
$env:PYTHONPATH  = "$PWD\.vega\venv\Lib\site-packages"
cargo build -p aether_decision --features pyo3
```

## Por que o script recusa `target/`

`tools/vega_env_setup.ps1` aborta (exit 3) se o venv pedido estiver sob
`target/`: a regra é do projeto, não do script — `target/` é do ciclo
automático, e o motor não pode ser dele.

## O que continua aberto (precisa do HITL)

A migração **custa rede e disco** (venv novo + torch ~2,5 GB) e mexe no lugar
onde o motor vive. A Fiada F1 está funcional dentro de `target/vega` hoje; a
migração é a transformação que a torna durável. O pedido explícito ao dono
está no fórum (MSG 0793/0794).

## Honestidade sobre o alcance

Isto **não** muda o comportamento do gate: mantém o mesmo motor, os mesmos
pins e as mesmas medições. O que muda é *onde* o motor mora e o fato de
existir um lockfile — para que a próxima limpeza do build não leve o AETHER
consigo.
