//! Port of the old host wakeup seam; no display, bus or data directory involved.
#![cfg(target_os = "linux")]
#[test]
#[allow(deprecated)]
fn worker_channel_wakes_blocked_glib_context() {
    use std::{cell::RefCell, rc::Rc, time::Duration};
    let context = glib::MainContext::new();
    let _guard = context.acquire().unwrap();
    let (send, receive) = glib::MainContext::channel(glib::Priority::DEFAULT);
    let got = Rc::new(RefCell::new(None));
    let output = got.clone();
    let _source = receive.attach(Some(&context), move |value| {
        *output.borrow_mut() = Some(value);
        glib::ControlFlow::Break
    });
    let timed_out = Rc::new(RefCell::new(false));
    let timeout = timed_out.clone();
    let deadline = glib::source::timeout_source_new(
        Duration::from_secs(2),
        Some("phase3-wakeup-deadline"),
        glib::Priority::DEFAULT,
        move || glib::ControlFlow::Break,
    );
    // Attach an independent deadline that wakes a stuck iteration; elapsed time
    // is checked below. The real transport must wake well before that deadline.
    let _deadline_id = deadline.attach(Some(&context));
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        send.send("ready").unwrap();
    });
    let start = std::time::Instant::now();
    while got.borrow().is_none() && start.elapsed() < Duration::from_secs(2) {
        context.iteration(true);
    }
    if got.borrow().is_none() {
        *timeout.borrow_mut() = true;
    }
    assert!(
        !*timed_out.borrow(),
        "blocked GLib context was never woken by worker channel"
    );
    assert_eq!(*got.borrow(), Some("ready"));
    deadline.destroy();

    worker.join().unwrap();
}
