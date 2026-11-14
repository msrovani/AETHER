//! Erros tipados do crate AETHER.

use std::fmt;

/// Resultado padrao do crate.
pub type Result<T> = std::result::Result<T, AetherError>;

/// Erros de dominio do motor.
#[derive(Debug, thiserror::Error)]
pub enum AetherError {
    /// Forma/dimensao de tensor incompativel com a operacao.
    #[error("shape mismatch: esperado {expected:?}, obtido {got:?}")]
    Shape {
        /// Forma esperada.
        expected: Vec<usize>,
        /// Forma obtida.
        got: Vec<usize>,
    },

    /// Datagrama UDP com tamanho diferente do protocolo de 40 bytes.
    #[error("datagrama invalido: {len} bytes (protocolo exige {expected})")]
    BadPacketLen {
        /// Tamanho recebido.
        len: usize,
        /// Tamanho exigido pelo protocolo.
        expected: usize,
    },

    /// Erro de sistema (socket, arquivo, temporizador).
    #[error("erro de io: {0}")]
    Io(#[from] std::io::Error),

    /// Parametro de configuracao invalido.
    #[error("configuracao invalida: {0}")]
    Config(String),

    /// Divisao por zero ou estado numerico degenerado.
    #[error("estado degenerado: {0}")]
    Degenerate(String),
}

impl AetherError {
    /// Constrói [`AetherError::Shape`] comparando esperado vs obtido.
    pub fn shape(expected: impl Into<Vec<usize>>, got: impl Into<Vec<usize>>) -> Self {
        Self::Shape {
            expected: expected.into(),
            got: got.into(),
        }
    }
}

/// Helper de formatacao para mensagens de estado (mantem `fmt` no escopo).
pub(crate) struct Fmt<'a>(pub &'a str);

impl fmt::Display for Fmt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
