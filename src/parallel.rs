use std::ops::ControlFlow;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;

pub fn map_each<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    job: impl Fn(&T) -> R + Sync,
    mut sink: impl FnMut(R) -> ControlFlow<()>,
) {
    debug_assert!(threads >= 1);
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..threads {
            let sender = sender.clone();
            let next = &next;
            let job = &job;
            scope.spawn(move || work(items, next, job, &sender));
        }
        drop(sender);
        receiver.iter().map(&mut sink).any(|flow| flow.is_break());
    });
}

fn work<T, R>(items: &[T], next: &AtomicUsize, job: &impl Fn(&T) -> R, sender: &Sender<R>) {
    while let Some(item) = items.get(next.fetch_add(1, Ordering::Relaxed)) {
        if sender.send(job(item)).is_err() {
            return;
        }
    }
}
