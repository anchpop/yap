// Build-time stand-in for yap-frontend-rs/pkg: the widget must never bundle
// the 4 MiB WASM module. Reused components use `import { type X }` from the
// pkg, which erases to a bare side-effect import — harmless against this
// empty module. A genuine VALUE import (e.g. `get_audio`) fails the build
// with "not exported by wasm-guard", which is exactly the alarm we want.
export {};
