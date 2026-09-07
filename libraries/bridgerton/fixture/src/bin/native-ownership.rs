//! Exercise ownership independently of Swift's tendency to retain `self` across await.
//! Run as an executable: native entry points intentionally reject Rust's worker test threads.
use bridge_fixture::{
    Card, Counter, ReviewState, bridgerton_counter_add_later, bridgerton_counter_card_later,
    bridgerton_counter_free, bridgerton_counter_new, bridgerton_counter_revise_card,
};
use bridgerton::{
    AbortController,
    native::{
        Bytes, FAILED, IntoResult, PENDING, bridgerton_buffer_free, bridgerton_task_free,
        bridgerton_task_poll, task,
    },
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
    task::Poll,
};

fn borrowed(bytes: &[u8]) -> Bytes {
    Bytes {
        data: bytes.as_ptr(),
        len: bytes.len(),
    }
}

fn data_ownership() {
    let monitor = Counter::new();
    let card = monitor.sample_card();
    let bytes = bridgerton::value::encode(&card).unwrap();
    let state = bridgerton::value::encode(&ReviewState::New).unwrap();
    unsafe {
        let object = bridgerton_counter_new().handle;
        let token = bridgerton::native::object(AbortController::new().signal()).handle;
        for end in 0..bytes.len() {
            let result =
                bridgerton_counter_revise_card(object, borrowed(&bytes[..end]), borrowed(&state));
            assert_eq!(result.status, FAILED);
            bridgerton_buffer_free(result.data);
        }
        let result =
            bridgerton_counter_revise_card(object, borrowed(&bytes), borrowed(&[0, 0, 0, 99]));
        assert_eq!(result.status, FAILED);
        bridgerton_buffer_free(result.data);
        let task = bridgerton_counter_card_later(object, borrowed(&bytes), token).handle;
        assert!(!task.is_null());
        // Free input allocation AND external object references before even polling.
        drop(bytes);
        assert_eq!(bridgerton_counter_free(object).status, 0);
        assert_eq!(
            bridgerton::native::call(|| {
                drop(Rc::from_raw(token.cast::<bridgerton::AbortSignal>()));
                Ok(().into_result())
            })
            .status,
            0
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            assert!(
                std::time::Instant::now() < deadline,
                "data task did not finish"
            );
            let result = bridgerton_task_poll(task, wake, 3);
            if result.status == PENDING {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
            assert_eq!(result.status, 0);
            let returned: Card = bridgerton::native::read_value(Bytes {
                data: result.data.data,
                len: result.data.len,
            })
            .unwrap();
            bridgerton_buffer_free(result.data);
            assert_eq!(returned.term, card.term);
            assert_eq!(returned.state, ReviewState::Known("remembered".into()));
            break;
        }
        assert_eq!(bridgerton_task_free(task).status, 0);
    }
    assert_eq!(monitor.live_counters(), 1);
    assert_eq!(monitor.active_operations(), 0);
    println!(
        "PASS: native rejects every truncated record; async owns decoded inputs after buffers and host handles are freed"
    );
}

extern "C" fn wake(_id: u64) {}

static WAKE_COUNT: AtomicU32 = AtomicU32::new(0);
extern "C" fn count_wake(_id: u64) {
    WAKE_COUNT.fetch_add(1, Ordering::SeqCst);
}

fn waker_outlives_future() {
    let confined = Rc::new(());
    let future_state = confined.clone();
    let captured_waker = Rc::new(RefCell::new(None));
    let capture = captured_waker.clone();
    let handle = task(async move {
        let _confined = future_state;
        std::future::poll_fn(move |context| {
            *capture.borrow_mut() = Some(context.waker().clone());
            Poll::<()>::Pending
        })
        .await;
        Ok(().into_result())
    })
    .handle;
    unsafe {
        assert_eq!(bridgerton_task_poll(handle, count_wake, 2).status, PENDING);
        assert_eq!(Rc::strong_count(&confined), 2);
        let waker = captured_waker.borrow_mut().take().unwrap();
        let early = waker.clone();
        std::thread::spawn(move || early.wake()).join().unwrap();
        assert_eq!(WAKE_COUNT.load(Ordering::SeqCst), 1);
        assert_eq!(bridgerton_task_free(handle).status, 0);
        assert_eq!(Rc::strong_count(&confined), 1);
        // This owns a real registered waker after the future and its !Send state are gone.
        std::thread::spawn(move || waker.wake()).join().unwrap();
        assert_eq!(WAKE_COUNT.load(Ordering::SeqCst), 1);
    }
}

fn moved_arguments() {
    let monitor = Counter::new();
    let baseline = monitor.live_counters();
    unsafe {
        let owner = bridgerton_counter_new().handle;
        let moved = bridgerton_counter_new().handle;
        // The first argument fails decoding, but the later owned object is still released.
        let result =
            bridge_fixture::bridgerton_counter_consume_with_card(owner, borrowed(&[]), moved);
        assert_eq!(result.status, FAILED);
        bridgerton_buffer_free(result.data);
        assert_eq!(monitor.live_counters(), baseline + 1);
        let first = bridgerton_counter_new().handle;
        let second = bridgerton_counter_new().handle;
        let held = bridgerton::native::borrow_object::<Counter>(first);
        // Cannot move out while Rust borrows the first object. The second must also be freed.
        let result = bridge_fixture::bridgerton_counter_try_consume_two(owner, first, second);
        assert_eq!(result.status, FAILED);
        bridgerton_buffer_free(result.data);
        assert_eq!(monitor.live_counters(), baseline + 2);
        drop(held);
        assert_eq!(monitor.live_counters(), baseline + 1);
        let moved = bridgerton_counter_new().handle;
        let pending = bridge_fixture::bridgerton_counter_consume_later(owner, moved).handle;
        assert_eq!(monitor.live_counters(), baseline + 2);
        assert_eq!(bridgerton_task_free(pending).status, 0);
        assert_eq!(monitor.live_counters(), baseline + 1);
        assert_eq!(bridgerton_counter_free(owner).status, 0);
        assert_eq!(monitor.live_counters(), baseline);
    }
    println!(
        "PASS: owned object arguments release on decode failure, failed moves, and early async destruction"
    );
}

// Deterministic interleavings, with real concurrent wakes and confined destructors.
fn lifecycle_stress() {
    struct ConfinedDrop(Rc<std::cell::Cell<usize>>);
    impl Drop for ConfinedDrop {
        fn drop(&mut self) {
            bridgerton::native::require_main_thread();
            self.0.set(self.0.get() + 1);
        }
    }
    let dropped = Rc::new(std::cell::Cell::new(0));
    let mut seed = 0x2a_90_bc_d1u32;
    for round in 0..2_000 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let state = ConfinedDrop(dropped.clone());
        let capture = Rc::new(RefCell::new(None));
        let captured = capture.clone();
        let mut polls = 0;
        let until_ready = seed % 8;
        let handle = task(async move {
            let _state = state;
            std::future::poll_fn(move |cx| {
                *captured.borrow_mut() = Some(cx.waker().clone());
                polls += 1;
                if polls > until_ready {
                    Poll::Ready(())
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            Ok(().into_result())
        })
        .handle;
        unsafe {
            if seed & 1 == 0 {
                assert_eq!(bridgerton_task_free(handle).status, 0);
            } else {
                let first = bridgerton_task_poll(handle, count_wake, round);
                let waker = capture.borrow_mut().take().unwrap();
                let background = waker.clone();
                let worker = std::thread::spawn(move || {
                    for _ in 0..128 {
                        background.wake_by_ref();
                    }
                });
                if first.status == PENDING && seed & 2 != 0 {
                    while bridgerton_task_poll(handle, count_wake, round).status == PENDING {}
                    let repeated = bridgerton_task_poll(handle, count_wake, round);
                    assert_eq!(repeated.status, FAILED);
                    bridgerton_buffer_free(repeated.data);
                    worker.join().unwrap();
                    let before = WAKE_COUNT.load(Ordering::SeqCst);
                    waker.wake_by_ref();
                    assert_eq!(WAKE_COUNT.load(Ordering::SeqCst), before);
                    assert_eq!(bridgerton_task_free(handle).status, 0);
                    assert_eq!(dropped.get(), round as usize + 1);
                    continue;
                }
                assert_eq!(bridgerton_task_free(handle).status, 0);
                worker.join().unwrap();
                let before = WAKE_COUNT.load(Ordering::SeqCst);
                waker.wake();
                assert_eq!(WAKE_COUNT.load(Ordering::SeqCst), before);
            }
        }
        assert_eq!(dropped.get(), round as usize + 1);
    }
    println!(
        "PASS: 2,000 randomized lifecycle schedules, racing/late wakes, and owner-thread destruction"
    );
}

fn main() {
    data_ownership();
    moved_arguments();
    waker_outlives_future();
    lifecycle_stress();
    let monitor = Counter::new();
    unsafe {
        let object = bridgerton_counter_new().handle;
        let token = bridgerton::native::object(AbortController::new().signal()).handle;
        let task = bridgerton_counter_add_later(object, 5, 50, token).handle;
        assert!(!task.is_null());
        assert_eq!(bridgerton_task_poll(task, wake, 1).status, PENDING);
        assert_eq!(monitor.active_operations(), 1);

        // The task must retain the receiver and decoded signal even after the host drops its references.
        assert_eq!(bridgerton_counter_free(object).status, 0);
        assert_eq!(
            bridgerton::native::call(|| {
                drop(Rc::from_raw(token.cast::<bridgerton::AbortSignal>()));
                Ok(().into_result())
            })
            .status,
            0
        );
        assert_eq!(monitor.live_counters(), 2);

        assert_eq!(bridgerton_task_free(task).status, 0);
        assert_eq!(monitor.active_operations(), 0);
        assert_eq!(monitor.live_counters(), 1);
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(monitor.active_operations(), 0);
    assert_eq!(monitor.live_counters(), 1);
    println!(
        "PASS: pending task retains objects and signals; early free drops confined state; retained waker is safe after future destruction"
    );
}
