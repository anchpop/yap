//! Run on the executable's main thread, as native ABI calls require.
#[cfg(target_vendor = "apple")]
fn main() {
    use bridgerton::native::{self, IntoResult, OK, PENDING};
    use std::{
        rc::Rc,
        thread,
        time::{Duration, Instant},
    };
    use tokio::runtime::{Builder, Handle};

    assert!(Handle::try_current().is_err());
    assert_eq!(native::call(|| Ok(1u32.into_result())).value, 1);
    assert!(
        Handle::try_current().is_err(),
        "ordinary calls create no runtime"
    );

    let host = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_time()
        .build()
        .unwrap();
    let expected = host.handle().id();
    native::set_tokio_handle(host.handle().clone()).unwrap();
    assert!(
        native::set_tokio_handle(host.handle().clone()).is_err(),
        "replacement must fail"
    );
    let owner = thread::current().id();
    let local = Rc::new(41u32);
    let task = native::call(|| {
        assert_eq!(Handle::current().id(), expected);
        let reentrant = native::call(|| {
            assert_eq!(Handle::current().id(), expected);
            Ok(().into_result())
        });
        assert_eq!(reentrant.status, OK);
        assert_eq!(
            Handle::current().id(),
            expected,
            "nested guard restores context"
        );
        Ok(native::task(async move {
            assert_eq!(Handle::current().id(), expected);
            assert_eq!(thread::current().id(), owner);
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert_eq!(
                Handle::current().id(),
                expected,
                "every poll enters host context"
            );
            assert_eq!(
                thread::current().id(),
                owner,
                "non-Send future stays on main thread"
            );
            Ok((*local + 1).into_result())
        }))
    });
    assert_eq!(task.status, OK);
    assert!(
        Handle::try_current().is_err(),
        "call restores previous context"
    );
    extern "C" fn wake(_: u64) {}
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = unsafe { native::bridgerton_task_poll(task.handle, wake, 0) };
        assert!(
            Handle::try_current().is_err(),
            "poll restores previous context"
        );
        if result.status != PENDING {
            assert_eq!(result.status, OK);
            assert_eq!(result.value, 42);
            break;
        }
        assert!(Instant::now() < deadline, "host timer must make progress");
        thread::sleep(Duration::from_millis(1));
    }
    let freed = unsafe { native::bridgerton_task_free(task.handle) };
    assert_eq!(freed.status, OK);
    // The host decides when to shut down, after all its confined work is done.
    drop(host);
    println!(
        "PASS: explicit host runtime, guarded reentry, main-thread async polls, and timer progress"
    );
}

#[cfg(not(target_vendor = "apple"))]
fn main() {}
