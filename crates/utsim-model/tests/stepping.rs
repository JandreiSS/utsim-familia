//! Phase 7 kernel contract: step-by-step runs, external stop/resume and decisions.

use utsim_core::task::MSG_DECISION;
use utsim_core::{
    Behavior, BlockId, Ctx, Dist, Item, ItemId, OutputRule, RunConfig, Simulation, Step,
};
use utsim_model::Model;
use utsim_objects::{Processor, Queue, Sink, Source};

const MM1: &str = r#"Model(
    name: "mm1",
    run: (stop_time: 2000.0, seed: 4),
    objects: [
        (name: "chegada", kind: Source(interarrival: Exp(mean: 1.0))),
        (name: "fila", kind: Queue()),
        (name: "maquina", kind: Processor(process: Exp(mean: 0.8))),
        (name: "saida", kind: Sink),
    ],
    connections: [("chegada", "fila"), ("fila", "maquina"), ("maquina", "saida")],
)"#;

#[test]
fn stepping_gives_the_same_result_as_run() {
    let m = Model::parse(MM1).unwrap();
    let direct = m.build(0).run().unwrap().metrics();
    let mut sim = m.build(0);
    let mut t = 0.0;
    loop {
        t += 37.5;
        match sim.run_until(t).unwrap() {
            Step::Paused(now) => assert_eq!(now, t),
            Step::Finished => break,
            Step::Decision(_) => unreachable!(),
        }
    }
    assert_eq!(sim.finish().metrics(), direct);
}

#[test]
fn external_stop_closes_the_machine() {
    let m = Model::parse(MM1).unwrap();
    let mut sim = m.build(0);
    sim.run_until(500.0).unwrap();
    let maquina = sim.block_id("maquina").unwrap();
    sim.stop_block(maquina);
    sim.run_until(1500.0).unwrap();
    let (_, state, _) = sim.block_status()[maquina.0].clone();
    assert_eq!(state, Some(utsim_core::State::Stopped));
    sim.resume_block(maquina);
    sim.run_until(2000.0).unwrap();
    let r = sim.finish();
    let down = r.block("maquina").unwrap().states["stopped"];
    assert!(
        (down - 0.5).abs() < 1e-9,
        "parada externa de 500 a 1500: {down}"
    );
    assert!(
        r.block("fila").unwrap().content_max > 100.0,
        "a fila acumula enquanto parada"
    );
}

/// Asks the agent which of two sinks each item goes to.
struct Chooser {
    asked: u64,
    waiting: Vec<ItemId>,
}

impl Behavior for Chooser {
    fn kind(&self) -> &'static str {
        "chooser"
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        true
    }

    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, _port: usize) {
        self.asked += 1;
        self.waiting.push(item);
        cx.request_decision(2, vec![cx.now(), self.asked as f64]);
    }

    fn on_message(&mut self, cx: &mut Ctx, _from: BlockId, tag: u32, payload: u64) {
        if tag == MSG_DECISION {
            let item = self.waiting.remove(0);
            cx.item_mut(item).set_label("porta", payload as f64 + 1.0);
            cx.release(item);
        }
    }
}

#[test]
fn decisions_pause_the_run_and_route_items() {
    let mut sim = Simulation::new(RunConfig {
        stop_time: 10.0,
        warmup: 0.0,
        seed: 1,
        replication: 0,
        sample_interval: None,
        histogram_bins: None,
    });
    let src = sim.add_block(
        "src",
        OutputRule::FirstAvailable,
        Box::new(Source::new(Dist::Const(1.0)).first_at_zero(true)),
    );
    let q = sim.add_block("q", OutputRule::FirstAvailable, Box::new(Queue::new(None)));
    let ch = sim.add_block(
        "agente",
        OutputRule::ByLabel("porta".into()),
        Box::new(Chooser {
            asked: 0,
            waiting: Vec::new(),
        }),
    );
    let a = sim.add_block("a", OutputRule::FirstAvailable, Box::new(Sink));
    let b = sim.add_block("b", OutputRule::FirstAvailable, Box::new(Sink));
    let p = sim.add_block(
        "p",
        OutputRule::FirstAvailable,
        Box::new(Processor::new(Dist::Const(0.0))),
    );
    let _ = p;
    sim.connect(src, q);
    sim.connect(q, ch);
    sim.connect(ch, a);
    sim.connect(ch, b);
    // Plain run() refuses: the model needs an agent.
    let mut asked = 0;
    loop {
        match sim.run_until(f64::INFINITY).unwrap() {
            Step::Decision(d) => {
                assert_eq!(d.options, 2);
                assert_eq!(d.observation[1], (asked + 1) as f64);
                // Even-numbered items to "a", odd to "b".
                sim.decide(asked % 2).unwrap();
                asked += 1;
            }
            Step::Finished => break,
            Step::Paused(_) => unreachable!(),
        }
    }
    assert!(sim.decide(0).is_err(), "não há decisão pendente");
    let r = sim.finish();
    assert_eq!(asked, 11);
    assert_eq!(r.block("a").unwrap().input, 6);
    assert_eq!(r.block("b").unwrap().input, 5);
}
