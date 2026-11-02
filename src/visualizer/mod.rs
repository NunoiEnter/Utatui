//! Audio spectrum source: runs cava with raw binary output and streams its frames.
//!
//! When cava is missing or dies, the app falls back to an idle animation that is
//! labelled as such in the UI — it never presents fake data as audio.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use crate::app::Event;
use crate::config::Visualizer;

/// Spawns cava and forwards each frame as `Event::Spectrum` (values in 0.0 – 1.0).
/// Sends `Event::VisualizerDown` with a reason if cava can't start or exits.
/// Aborting the returned task stops cava.
pub fn spawn(cfg: &Visualizer, tx: UnboundedSender<Event>) -> tokio::task::JoinHandle<()> {
    let bars = cfg.bars.clamp(2, 512) as usize;
    let conf = format!(
        "[general]\nbars = {bars}\nframerate = {}\nautosens = 1\nsensitivity = {}\n\
         [output]\nmethod = raw\nraw_target = /dev/stdout\ndata_format = binary\n\
         bit_format = 16bit\nchannels = mono\nmono_option = average\n\
         [smoothing]\nnoise_reduction = {}\n",
        cfg.framerate.clamp(1, 240),
        cfg.sensitivity.max(1),
        cfg.noise_reduction.min(100),
    );
    let cava = cfg.cava_path.clone();
    tokio::spawn(async move {
        let reason = match run(&cava, &conf, bars, &tx).await {
            Ok(()) => "cava exited".to_string(),
            Err(e) => format!("cava unavailable: {e}"),
        };
        let _ = tx.send(Event::VisualizerDown(reason));
    })
}

async fn run(cava: &str, conf: &str, bars: usize, tx: &UnboundedSender<Event>) -> std::io::Result<()> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!("utatui-cava-{}.conf", std::process::id()));
    tokio::fs::write(&path, conf).await?;
    let child = Command::new(cava)
        .arg("-p")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(e);
        }
    };
    let mut out = child.stdout.take().expect("stdout is piped");
    let mut buf = vec![0u8; bars * 2];
    // cava reads its config at startup; once the first frame arrives the file can go.
    let first = out.read_exact(&mut buf).await;
    let _ = tokio::fs::remove_file(&path).await;
    first?;
    loop {
        let frame = buf
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_ne_bytes([b[0], b[1]]) as f32 / 65535.0)
            .collect();
        if tx.send(Event::Spectrum(frame)).is_err() {
            return Ok(()); // app is gone; kill_on_drop stops cava
        }
        out.read_exact(&mut buf).await?;
    }
}

/// Idle animation shown when no audio capture is available: a slow, low rolling wave.
pub fn idle_frame(bars: usize, start: Instant) -> Vec<f32> {
    let t = start.elapsed().as_secs_f32();
    (0..bars)
        .map(|i| {
            let x = i as f32 / bars.max(1) as f32;
            0.08 + 0.06 * ((x * 9.0 - t * 1.6).sin() + (x * 4.0 + t * 0.9).sin()) * 0.5 + 0.06
        })
        .collect()
}
