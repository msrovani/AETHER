//! F1 — diagnóstico do interpretador embutido (PyO3 `auto-initialize`).
//!
//! Pergunta: o interpreter EMBUTIDO enxerga o `site-packages` do venv
//! (`vegaml`)? O smoke `f1_smoke` abstém com
//! `ModuleNotFoundError: No module named 'vegaml'` — ou seja, o embed roda,
//! mas o `sys.path` do processo não inclui o venv.
//!
//! Este probe imprime o que o interpreter embutido enxerga (executável,
//! prefix, base_prefix, site-packages, `sys.path`) e testa o
//! `import vegaml` antes e depois de inserir o `site-packages` do venv.
//!
//! ```text
//! PYO3_PYTHON=target/vega/Scripts/python.exe \
//!   cargo run -p aether_decision --features pyo3 --example f1_env_probe
//! ```

use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};

fn main() {
    let venv_site = std::env::var("AETHER_VEGA_SITE_PACKAGES")
        .unwrap_or_else(|_| "target/vega/Lib/site-packages".into());

    Python::attach(|py| {
        let sys = py.import("sys").expect("sys");
        let str_of = |v: &Bound<'_, PyAny>| -> String {
            v.str()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        let attr = |k: &str| -> String {
            sys.getattr(k)
                .map(|v| str_of(&v))
                .unwrap_or_else(|_| "<erro>".into())
        };
        println!("executable    = {}", attr("executable"));
        println!("prefix        = {}", attr("prefix"));
        println!("base_prefix   = {}", attr("base_prefix"));
        println!("version       = {}", attr("version"));

        let site_pkgs = sys
            .getattr("site")
            .ok()
            .and_then(|s| s.getattr("getsitepackages").ok())
            .and_then(|f| f.call0().ok())
            .map(|v| str_of(&v))
            .unwrap_or_else(|| "<erro>".into());
        println!("site-packages = {site_pkgs}");

        if let Ok(path) = sys.getattr("path") {
            if let Ok(list) = path.cast::<PyList>() {
                println!("sys.path:");
                for p in list.iter() {
                    println!("  - {}", str_of(&p));
                }
            }
        }

        // Teste 1: importa direto (como está).
        match py.import("vegaml") {
            Ok(_) => println!("import vegaml (sem ajuste)  = OK"),
            Err(e) => println!("import vegaml (sem ajuste)  = FALHOU: {e}"),
        }

        // Teste 2: insere o site-packages do venv e tenta de novo.
        if std::path::Path::new(&venv_site).exists() {
            let item = PyString::new(py, &venv_site);
            if let Ok(path) = sys.getattr("path") {
                if let Ok(list) = path.cast::<PyList>() {
                    match list.insert(0, item) {
                        Ok(()) => println!("insert site-packages        = OK"),
                        Err(e) => println!("insert site-packages        = FALHOU: {e}"),
                    }
                }
            }
            match py.import("vegaml") {
                Ok(m) => {
                    let v = m
                        .getattr("__version__")
                        .ok()
                        .and_then(|v| v.extract::<String>().ok());
                    println!("import vegaml (com ajuste)  = OK  versao={v:?}");
                    match py.import("torch") {
                        Ok(t) => {
                            let tv = t
                                .getattr("__version__")
                                .ok()
                                .and_then(|v| v.extract::<String>().ok());
                            let cuda = t
                                .getattr("cuda")
                                .ok()
                                .and_then(|c| c.getattr("is_available").ok())
                                .and_then(|f| f.call0().ok())
                                .and_then(|v| v.extract::<bool>().ok());
                            println!("torch={tv:?} cuda={cuda:?}");
                        }
                        Err(e) => println!("torch = NAO IMPORTADO: {e}"),
                    }
                }
                Err(e) => println!("import vegaml (com ajuste)  = FALHOU: {e}"),
            }
        } else {
            println!("site-packages do venv NAO EXISTE: {venv_site}");
        }
    });
}
