//! Phase 6 kernel contract: subscriptions, item injection and object scripts,
//! exercised with test-only blocks (the flow engine and Rhai scripts are fronts).

use utsim_core::script::{ItemScript, PortScript, ScriptEnv};
use utsim_core::task::{MSG_ITEM_ENTERED, MSG_ITEM_EXITED};
use utsim_core::{
    Behavior, BlockId, Ctx, Dist, Item, ItemId, OutputRule, RunConfig, Simulation, Value,
};
use utsim_objects::{Processor, Queue, Sink, Source};

/// Listens to a block and, at t = 1, 2, 3, injects an item into another one.
struct Watcher {
    watched: BlockId,
    target: BlockId,
    entered: u64,
    exited: u64,
}

impl Behavior for Watcher {
    fn kind(&self) -> &'static str {
        "watcher"
    }

    fn reset(&mut self, cx: &mut Ctx) {
        cx.subscribe(self.watched, true, true);
        for t in [1.0, 2.0, 3.0] {
            cx.schedule(t, 0, None);
        }
    }

    fn accepts(&self, _item: &Item, _content: usize, _port: usize) -> bool {
        false
    }

    fn on_event(&mut self, cx: &mut Ctx, _tag: u32, _item: Option<ItemId>) {
        let id = cx.inject_item(self.target, 0);
        cx.item_mut(id).set_label("origem", "fluxo");
    }

    fn on_message(&mut self, cx: &mut Ctx, from: BlockId, tag: u32, payload: u64) {
        assert_eq!(from, self.watched);
        let item = cx.item_by_key(payload);
        match tag {
            MSG_ITEM_ENTERED => {
                assert!(item.is_some(), "o item que entrou existe");
                self.entered += 1;
                cx.record("vistos_entrando", 1.0);
            }
            MSG_ITEM_EXITED => {
                self.exited += 1;
                cx.record("vistos_saindo", 1.0);
            }
            _ => {}
        }
    }
}

/// Marks every entering item with the time it arrived.
struct Stamp;

impl ItemScript for Stamp {
    fn run(&mut self, item: &mut Item, env: &mut ScriptEnv) -> Result<(), String> {
        item.set_label("chegou", env.now);
        Ok(())
    }
}

/// Sends items whose `chegou` is below 5 to port 0, the rest to port 1.
struct Split;

impl PortScript for Split {
    fn port(&mut self, item: &Item, outputs: usize, _env: &mut ScriptEnv) -> Result<usize, String> {
        assert_eq!(outputs, 2);
        let t = item
            .label("chegou")
            .and_then(Value::as_num)
            .ok_or("sem rótulo")?;
        Ok(if t < 5.0 { 0 } else { 1 })
    }
}

fn config(stop: f64) -> RunConfig {
    RunConfig {
        stop_time: stop,
        warmup: 0.0,
        seed: 1,
        replication: 0,
        sample_interval: None,
        histogram_bins: None,
    }
}

#[test]
fn subscriptions_and_injection_reach_other_blocks() {
    let mut sim = Simulation::new(config(10.0));
    let src = sim.add_block(
        "src",
        OutputRule::FirstAvailable,
        Box::new(Source::new(Dist::Const(1.0)).first_at_zero(true)),
    );
    let q = sim.add_block("q", OutputRule::FirstAvailable, Box::new(Queue::new(None)));
    let p = sim.add_block(
        "p",
        OutputRule::FirstAvailable,
        Box::new(Processor::new(Dist::Const(0.5)).capacity(10)),
    );
    let out = sim.add_block("out", OutputRule::FirstAvailable, Box::new(Sink));
    let watcher = sim.add_block(
        "watcher",
        OutputRule::FirstAvailable,
        Box::new(Watcher {
            watched: q,
            target: p,
            entered: 0,
            exited: 0,
        }),
    );
    let _ = watcher;
    sim.connect(src, q);
    sim.connect(q, p);
    sim.connect(p, out);
    let r = sim.run().unwrap();
    // 11 items enter (t = 0..=10) and leave the queue at once; the watcher sees them all.
    let w = r.block("watcher").unwrap();
    assert_eq!(w.extra["vistos_entrando"].count, 11);
    assert_eq!(w.extra["vistos_saindo"].count, 11);
    // 3 injected items reach the processor besides the 11 from the queue.
    assert_eq!(r.block("p").unwrap().input, 14);
    // The item that arrives at t = 10 finishes at 10.5, after the end: 13 reach the sink.
    assert_eq!(r.block("out").unwrap().input, 13);
}

#[test]
fn entry_and_port_scripts_run_in_the_kernel() {
    let mut sim = Simulation::new(config(9.5));
    let src = sim.add_block(
        "src",
        OutputRule::FirstAvailable,
        Box::new(Source::new(Dist::Const(1.0)).first_at_zero(true)),
    );
    let q = sim.add_block(
        "q",
        OutputRule::Script("split".into()),
        Box::new(Queue::new(None)),
    );
    let early = sim.add_block("cedo", OutputRule::FirstAvailable, Box::new(Sink));
    let late = sim.add_block("tarde", OutputRule::FirstAvailable, Box::new(Sink));
    sim.set_entry_script(q, Box::new(Stamp));
    sim.set_port_script(q, Box::new(Split));
    sim.connect(src, q);
    sim.connect(q, early);
    sim.connect(q, late);
    let r = sim.run().unwrap();
    // Arrivals at 0..=9: times 0..4 go early (5 items), 5..9 late (5 items).
    assert_eq!(r.block("cedo").unwrap().input, 5);
    assert_eq!(r.block("tarde").unwrap().input, 5);
}
