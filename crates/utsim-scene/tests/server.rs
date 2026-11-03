//! The socket server: TCP and WebSocket clients, back-pressure, commands.
//! Everything binds to 127.0.0.1 with port 0.
#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use utsim_scene::server::SceneServer;
use utsim_scene::{
    CommandValue, EntityId, Frame, Header, Motion, PROTOCOL, Pose, SceneSink, StaticScene,
};

fn header() -> Header {
    Header {
        protocol: PROTOCOL.to_string(),
        model: "teste".into(),
        units: "m".into(),
        up: "z".into(),
        start: 0.0,
        end: 10.0,
        scene: StaticScene::default(),
    }
}

fn frame(t: f64, poses: usize) -> Frame {
    Frame {
        time: t,
        poses: (0..poses)
            .map(|i| Pose {
                id: EntityId::Item(i as u64),
                position: [t as f32, i as f32, 0.0],
                yaw: 0.0,
                motion: Motion::Idle,
                lift: 0.0,
                parent: None,
                state: None,
                fill: None,
                item_type: None,
            })
            .collect(),
    }
}

fn tcp(server: &SceneServer) -> BufReader<TcpStream> {
    let s = TcpStream::connect(server.local_addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    BufReader::new(s)
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !ok() {
        assert!(Instant::now() < end, "timeout: {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn line(r: &mut BufReader<TcpStream>) -> serde_json::Value {
    let mut s = String::new();
    r.read_line(&mut s).unwrap();
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("{e}: {s:?}"))
}

#[test]
fn several_clients_get_the_header_then_frames() {
    let mut server = SceneServer::bind("127.0.0.1:0", &header()).unwrap();
    let mut a = tcp(&server);
    let mut b = tcp(&server);
    let mut ws = tungstenite::connect(format!("ws://{}", server.local_addr()))
        .unwrap()
        .0;
    wait_for("3 clients", || server.clients() == 3);
    for k in 0..3 {
        server.frame(&frame(k as f64, 2)).unwrap();
    }
    for r in [&mut a, &mut b] {
        let h = line(r);
        assert_eq!(h["protocol"], PROTOCOL);
        assert_eq!(h["model"], "teste");
        for k in 0..3 {
            let f = line(r);
            assert_eq!(f["time"], k as f64);
            assert_eq!(f["poses"].as_array().unwrap().len(), 2);
        }
    }
    // The WebSocket client gets the same messages, one per text message.
    let first = ws.read().unwrap().into_text().unwrap();
    let h: serde_json::Value = serde_json::from_str(first.as_str()).unwrap();
    assert_eq!(h["protocol"], PROTOCOL);
    let second = ws.read().unwrap().into_text().unwrap();
    let f: Frame = serde_json::from_str(second.as_str()).unwrap();
    assert_eq!(f.time, 0.0);
}

#[test]
fn websocket_handshake_and_command_back() {
    let server = SceneServer::bind("127.0.0.1:0", &header()).unwrap();
    let (mut ws, response) = tungstenite::connect(format!("ws://{}", server.local_addr())).unwrap();
    assert_eq!(response.status(), 101);
    let first = ws.read().unwrap().into_text().unwrap();
    assert!(first.as_str().contains(PROTOCOL));
    ws.send(tungstenite::Message::text(
        r#"{"tipo":"comando","variavel":"M1.Parar","valor":true}"#,
    ))
    .unwrap();
    wait_for("command", || {
        let c = server.take_commands();
        if c.is_empty() {
            return false;
        }
        assert_eq!(c[0].variable, "M1.Parar");
        assert_eq!(c[0].value, CommandValue::Bool(true));
        true
    });
}

#[test]
fn tcp_commands_unknown_types_and_garbage() {
    let server = SceneServer::bind("127.0.0.1:0", &header()).unwrap();
    let mut r = tcp(&server);
    line(&mut r); // header
    let w = r.get_mut();
    // An unknown type is ignored; a speed command is a number.
    w.write_all(b"{\"tipo\":\"futuro\",\"x\":1}\n").unwrap();
    w.write_all(b"{\"tipo\":\"comando\",\"variavel\":\"Velocidade\",\"valor\":4.0}\n")
        .unwrap();
    // Broken JSON is answered with a warning.
    w.write_all(b"isto nao e json\n").unwrap();
    let mut got = Vec::new();
    wait_for("command", || {
        got.extend(server.take_commands());
        !got.is_empty()
    });
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].value, CommandValue::Num(4.0));
    let warn = line(&mut r);
    assert_eq!(warn["tipo"], "aviso");
}

/// A client that never reads must not stall the producer or the other clients.
#[test]
fn a_client_that_never_reads_does_not_stall_the_others() {
    let mut server = SceneServer::bind("127.0.0.1:0", &header()).unwrap();
    let stuck = TcpStream::connect(server.local_addr()).unwrap(); // never read
    let mut good = tcp(&server);
    wait_for("2 clients", || server.clients() == 2);
    let started = Instant::now();
    // About 40 MB: far more than the socket buffers and the queue of the stuck client.
    let total = 200;
    let reader = std::thread::spawn(move || {
        line(&mut good); // header
        let mut seen = 0;
        let mut last = -1.0;
        // A busy client may lose frames too, but it keeps receiving them, in order.
        while last < (total - 1) as f64 {
            let t = line(&mut good)["time"].as_f64().unwrap();
            assert!(t > last, "frames arrive in order");
            last = t;
            seen += 1;
        }
        seen
    });
    let mut k = 0;
    while !reader.is_finished() {
        let t = k.min(total - 1) as f64;
        server.frame(&frame(t, 2500)).unwrap();
        k += 1;
        std::thread::sleep(Duration::from_millis(3));
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "producer or good client stalled"
        );
    }
    assert!(reader.join().unwrap() >= 5);
    assert!(
        server.dropped() > 0,
        "the stuck client should have lost frames"
    );
    drop(stuck);
}

#[test]
fn dropping_the_server_closes_clients_after_the_queue_is_sent() {
    let mut server = SceneServer::bind("127.0.0.1:0", &header()).unwrap();
    let mut r = tcp(&server);
    wait_for("client", || server.clients() == 1);
    server.frame(&frame(1.0, 1)).unwrap();
    drop(server);
    line(&mut r);
    assert_eq!(line(&mut r)["time"], 1.0);
    let mut rest = String::new();
    assert_eq!(r.read_line(&mut rest).unwrap(), 0, "connection closed");
}
