//! `UniFFI`'s binding generator, built from the pinned `uniffi` version so the Swift and Kotlin it writes match this
//! crate's scaffolding: `cargo run -p ome-ffi --features bindgen --bin uniffi-bindgen -- generate --library …`.

fn main() {
    uniffi::uniffi_bindgen_main();
}
