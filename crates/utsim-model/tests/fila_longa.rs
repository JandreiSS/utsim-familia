//! A long FIFO queue in front of a busy machine: the kernel asks the machine whether it
//! takes an item about once per move, not once per waiting item (that was quadratic in
//! the queue length), and the result is the same as asking for every item.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use utsim_core::{Behavior, Ctx, Dist, Item, ItemId, OutputRule, RunConfig, Simulation};
use utsim_objects::{Processor, Queue, Sink, Source};

/// A processor that counts the calls to `accepts` and can hide that its answer does not
/// depend on the item (the kernel then asks for every waiting item, as it used to).
struct Counted {
    inner: Processor,
    calls: Arc<AtomicUsize>,
    item_independent: bool,
}

impl Behavior for Counted {
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }
    fn reset(&mut self, cx: &mut Ctx) {
        self.inner.reset(cx);
    }
    fn accepts(&self, item: &Item, content: usize, port: usize) -> bool {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.accepts(item, content, port)
    }
    fn accepts_is_item_independent(&self) -> bool {
        self.item_independent && self.inner.accepts_is_item_independent()
    }
    fn on_entry(&mut self, cx: &mut Ctx, item: ItemId, port: usize) {
        self.inner.on_entry(cx, item, port);
    }
    fn on_exit(&mut self, cx: &mut Ctx, item: ItemId) {
        self.inner.on_exit(cx, item);
    }
    fn on_event(&mut self, cx: &mut Ctx, tag: u32, item: Option<ItemId>) {
        self.inner.on_event(cx, tag, item);
    }
    fn on_stop(&mut self, cx: &mut Ctx) {
        self.inner.on_stop(cx);
    }
    fn on_resume(&mut self, cx: &mut Ctx) {
        self.inner.on_resume(cx);
    }
    fn on_warmup(&mut self, cx: &mut Ctx) {
        self.inner.on_warmup(cx);
    }
    fn work_in_progress(&self, cx: &Ctx) -> bool {
        self.inner.work_in_progress(cx)
    }
}

/// One item per time unit into a queue, a machine that takes two: after `stop` time
/// units about `stop / 2` items are waiting. Returns the report and the `accepts` calls.
fn run(stop: f64, item_independent: bool) -> (utsim_core::RunReport, usize) {
    let mut sim = Simulation::new(RunConfig {
        stop_time: stop,
        warmup: 0.0,
        seed: 11,
        replication: 0,
        sample_interval: None,
        histogram_bins: None,
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let src = sim.add_block(
        "chegada",
        OutputRule::FirstAvailable,
        Box::new(Source::new(Dist::Const(1.0)).first_at_zero(true)),
    );
    let fila = sim.add_block(
        "fila",
        OutputRule::FirstAvailable,
        Box::new(Queue::new(None)),
    );
    let maquina = sim.add_block(
        "maquina",
        OutputRule::FirstAvailable,
        Box::new(Counted {
            inner: Processor::new(Dist::Uniform { min: 1.5, max: 2.5 }),
            calls: calls.clone(),
            item_independent,
        }),
    );
    let saida = sim.add_block("saida", OutputRule::FirstAvailable, Box::new(Sink));
    sim.connect(src, fila);
    sim.connect(fila, maquina);
    sim.connect(maquina, saida);
    let report = sim.run().unwrap();
    (report, calls.load(Ordering::Relaxed))
}

#[test]
fn long_queue_asks_the_machine_once_per_move() {
    let (r, calls) = run(10_000.0, true);
    let waiting = r.block("fila").unwrap().content_max;
    assert!(
        waiting > 4_900.0,
        "a fila deveria passar de 4.900: {waiting}"
    );
    // About 10.000 arrivals and 5.000 departures: a few calls per move, where asking
    // per waiting item would make about 37 million.
    assert!(calls < 60_000, "chamadas a accepts: {calls}");
}

#[test]
fn skipping_refusals_gives_the_same_run() {
    let (fast, fast_calls) = run(3_000.0, true);
    let (slow, slow_calls) = run(3_000.0, false);
    assert_eq!(fast.metrics(), slow.metrics());
    assert_eq!(fast.events, slow.events);
    assert!(
        fast_calls * 50 < slow_calls,
        "{fast_calls} contra {slow_calls}"
    );
}
