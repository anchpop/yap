use bridgerton::platform::{init_logging, run_blocking, sleep_ms};

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn blocking_work_leaves_the_executor_free_to_run_timers() {
    let (send, receive) = std::sync::mpsc::channel();
    let work = run_blocking(move || receive.recv_timeout(std::time::Duration::from_secs(5)));
    let timer = async {
        sleep_ms(1).await;
        send.send(42).unwrap();
    };
    let (result, ()) = tokio::join!(work, timer);
    assert_eq!(result.unwrap().unwrap(), 42);
    // A closure's domain error remains separate from a task-join error.
    assert_eq!(
        run_blocking(|| Err::<(), _>("invalid data")).await.unwrap(),
        Err("invalid data")
    );
    init_logging();
    init_logging();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
async fn browser_helpers_work_without_a_native_runtime() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    // Browser blocking work completes inline when polled.
    let mut work = Box::pin(run_blocking(|| 42));
    assert!(matches!(
        work.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(42))
    ));
    let _timer = bridgerton::platform::PerfTimer::new("helper smoke");
    init_logging();
    init_logging();
    sleep_ms(1).await;
    assert!(bridgerton::platform::runtime_env("PATH").is_none());
}
