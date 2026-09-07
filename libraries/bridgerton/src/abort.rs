//! Browser signals retain their browser ABI. Native signals are owned bridge handles.
#[cfg(target_arch = "wasm32")]
pub use abort_signal::{AbortController, AbortSignal};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use crate::bridge;
    use crate::{
        Error,
        native::{self, NativeArgument, NativeOptionalArgument},
        schema::{NativeType, Registry},
    };
    use std::ffi::c_void;

    #[derive(Clone)]
    #[bridge(opaque, custom_arguments)]
    pub struct AbortSignal(abort_signal::AbortSignal);

    #[bridge]
    impl AbortSignal {
        pub fn aborted(&self) -> bool {
            self.0.aborted()
        }
    }
    impl AbortSignal {
        pub async fn cancelled(&self) {
            self.0.cancelled().await
        }
        pub async fn until<T>(
            &self,
            future: impl std::future::Future<Output = T>,
        ) -> Result<T, abort_signal::Aborted> {
            self.0.until(future).await
        }
    }

    impl From<AbortSignal> for abort_signal::AbortSignal {
        fn from(signal: AbortSignal) -> Self {
            signal.0
        }
    }

    #[bridge(opaque)]
    pub struct AbortController(abort_signal::AbortController);
    #[bridge]
    impl AbortController {
        #[bridge(constructor)]
        pub fn new() -> Self {
            Self(abort_signal::AbortController::default())
        }
        pub fn abort(&self) {
            self.0.abort();
        }
        pub fn signal(&self) -> AbortSignal {
            AbortSignal(self.0.signal())
        }
        /// A child follows its parent; aborting it never aborts the parent or siblings.
        pub fn child_of(signal: Option<AbortSignal>) -> Self {
            signal.map_or_else(Self::new, |signal| {
                Self(abort_signal::AbortController::child_of(&signal.0))
            })
        }
    }
    impl Default for AbortController {
        fn default() -> Self {
            Self::new()
        }
    }
    impl NativeArgument for AbortSignal {
        type Abi = *const c_void;
        type Prepared = *const c_void;
        const C_TYPE: &'static str = "const void *";
        unsafe fn prepare_native(handle: Self::Abi) -> Self::Prepared {
            handle
        }
        unsafe fn from_prepared(handle: Self::Prepared) -> Result<Self, Error> {
            if handle.is_null() {
                return Err(Error::new("missing abort signal"));
            }
            Ok((*unsafe { native::borrow_object::<Self>(handle) }).clone())
        }
        fn swift_type(registry: &mut Registry) -> String {
            Self::native_type(registry).swift()
        }
        fn argument(name: &str) -> String {
            format!("{name}.__bridgertonHandle")
        }
        fn wrap(name: &str, call: String) -> String {
            format!("withExtendedLifetime({name}) {{ {call} }}")
        }
        fn task_signal(name: &str) -> Option<String> {
            Some(format!(
                "        let __abort_{name} = AbortController.child_of(signal: {name})\n        let {name} = __abort_{name}.signal()\n"
            ))
        }
    }
    impl NativeOptionalArgument for AbortSignal {
        type Abi = *const c_void;
        type Prepared = *const c_void;
        const C_TYPE: &'static str = "const void *";
        unsafe fn prepare_optional(handle: Self::Abi) -> Self::Prepared {
            handle
        }
        unsafe fn from_optional(handle: Self::Prepared) -> Result<Option<Self>, Error> {
            if handle.is_null() {
                Ok(None)
            } else {
                unsafe { AbortSignal::from_prepared(handle) }.map(Some)
            }
        }
        fn optional_type(registry: &mut Registry) -> String {
            format!("{}? = nil", AbortSignal::native_type(registry).swift())
        }
        fn optional_argument(name: &str) -> String {
            format!("{name}?.__bridgertonHandle")
        }
        fn optional_wrap(name: &str, call: String) -> String {
            format!("withExtendedLifetime({name}) {{ {call} }}")
        }
        fn optional_task_signal(name: &str) -> Option<String> {
            Some(format!(
                "        let __abort_{name} = AbortController.child_of(signal: {name})\n        let {name}: AbortSignal? = __abort_{name}.signal()\n"
            ))
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use native::{AbortController, AbortSignal};
