//! Uma tarefa pesada (simulação) num fio de trabalho. A tela chama [`Trabalho::sondar`] a
//! cada quadro e recebe o resultado quando ele chega.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// Resultado de uma tarefa: o valor ou a mensagem de erro.
pub type Resultado<T> = Result<T, String>;

pub struct Trabalho<T> {
    rx: Option<Receiver<Resultado<T>>>,
    /// Último resultado recebido.
    pub resultado: Option<Resultado<T>>,
}

impl<T> Default for Trabalho<T> {
    fn default() -> Self {
        Trabalho {
            rx: None,
            resultado: None,
        }
    }
}

impl<T: Send + 'static> Trabalho<T> {
    /// Começa a tarefa (descarta o resultado anterior). Ignora se já há uma em curso.
    pub fn iniciar(&mut self, f: impl FnOnce() -> Resultado<T> + Send + 'static) {
        if self.rx.is_some() {
            return;
        }
        self.resultado = None;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        self.rx = Some(rx);
    }
}

impl<T> Trabalho<T> {
    pub fn ocupado(&self) -> bool {
        self.rx.is_some()
    }

    /// Recolhe o resultado se já chegou. Devolve `true` no quadro em que ele chega.
    pub fn sondar(&mut self, ctx: &egui::Context) -> bool {
        let Some(rx) = &self.rx else {
            return false;
        };
        match rx.try_recv() {
            Ok(r) => {
                self.resultado = Some(r);
                self.rx = None;
                true
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.resultado = Some(Err("a tarefa terminou sem resultado".into()));
                self.rx = None;
                true
            }
        }
    }

    pub fn ok(&self) -> Option<&T> {
        self.resultado.as_ref().and_then(|r| r.as_ref().ok())
    }

    pub fn erro(&self) -> Option<&str> {
        self.resultado
            .as_ref()
            .and_then(|r| r.as_ref().err())
            .map(String::as_str)
    }
}
