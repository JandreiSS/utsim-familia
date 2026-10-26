//! Minimal client of the live scene (`utsim-scene/1` over plain TCP).
//!
//! ```text
//! utsim emular examples/tutorial_02_linha.ron --cena-porta 7878
//! cargo run -p utsim-scene --features server --example cliente_cena -- 127.0.0.1:7878
//! cargo run -p utsim-scene --features server --example cliente_cena -- 127.0.0.1:7878 --parar furacao
//! ```
//!
//! Prints a summary of the header, then one frame per second of wall time. With
//! `--parar BLOCO` it sends a stop command two seconds after connecting and resumes the
//! block two seconds later, which is what a "button" in a VR scene would do.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use utsim_scene::{ClientMessage, CommandValue, EntityKind, Frame, Header};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut addr = "127.0.0.1:7878".to_string();
    let mut stop_block = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--parar" => stop_block = args.next(),
            _ => addr = a,
        }
    }
    let stream = TcpStream::connect(&addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let mut text = String::new();
    reader.read_line(&mut text)?;
    reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(200)))?;
    let header: Header = serde_json::from_str(&text)?;
    let mobiles = header
        .scene
        .entities
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Mobile { .. }))
        .count();
    println!(
        "{}: modelo \"{}\", {} entidades ({} móveis), {} segmentos, t de {} a {}",
        header.protocol,
        header.model,
        header.scene.entities.len(),
        mobiles,
        header.scene.network.len(),
        header.start,
        header.end
    );

    let (started, mut last_print) = (Instant::now(), Instant::now());
    let (mut frames, mut sent_stop, mut sent_resume) = (0u64, false, false);
    let mut line = String::new();
    loop {
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                // A partial line stays in `line` until the rest arrives.
                continue_timers(
                    &started,
                    &stop_block,
                    &mut writer,
                    &mut sent_stop,
                    &mut sent_resume,
                )?;
                continue;
            }
            Err(e) => return Err(e.into()),
        }
        continue_timers(
            &started,
            &stop_block,
            &mut writer,
            &mut sent_stop,
            &mut sent_resume,
        )?;
        let text = std::mem::take(&mut line);
        let value: serde_json::Value = serde_json::from_str(&text)?;
        // Extension messages carry "tipo"; readers ignore the ones they do not know.
        if let Some(tipo) = value.get("tipo").and_then(|t| t.as_str()) {
            println!("[{tipo}] {}", text.trim());
            if tipo == "fim" {
                break;
            }
            continue;
        }
        let frame: Frame = serde_json::from_value(value)?;
        frames += 1;
        if last_print.elapsed() >= Duration::from_secs(1) {
            last_print = Instant::now();
            let stopped: Vec<String> = frame
                .poses
                .iter()
                .filter(|p| p.state.as_deref() == Some("stopped"))
                .map(|p| format!("{:?}", p.id))
                .collect();
            println!(
                "t={:.1}: {} poses, {} quadros recebidos, parados: {:?}",
                frame.time,
                frame.poses.len(),
                frames,
                stopped
            );
        }
    }
    println!("conexão encerrada após {frames} quadros");
    Ok(())
}

/// Sends the stop command 2 s after connecting and the resume 2 s later.
fn continue_timers(
    started: &Instant,
    block: &Option<String>,
    out: &mut TcpStream,
    sent_stop: &mut bool,
    sent_resume: &mut bool,
) -> std::io::Result<()> {
    let Some(block) = block else { return Ok(()) };
    let elapsed = started.elapsed();
    for (due, flag, on) in [
        (Duration::from_secs(2), sent_stop, true),
        (Duration::from_secs(4), sent_resume, false),
    ] {
        if !*flag && elapsed >= due {
            *flag = true;
            let msg = ClientMessage::Comando {
                variavel: format!("{block}.Parar"),
                valor: CommandValue::Bool(on),
            };
            let mut text = serde_json::to_string(&msg)?;
            text.push('\n');
            out.write_all(text.as_bytes())?;
            println!("comando enviado: {}", text.trim());
        }
    }
    Ok(())
}
