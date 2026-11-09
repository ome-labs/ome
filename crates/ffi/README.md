# ome-ffi

The phone core of an Ome member's signing device, exposed to Swift and Kotlin through UniFFI.

A phone holds the member's signing key; the member's keeper and daemon keep running on a desktop or their own server.
The phone pairs with the keeper through a one-time offer, receives proposals to review through the relay mailbox,
checks everything it can itself (the committee, the calls it signs, each call's function, its own slot, its policy, a
rotation's shape) and signs only then. It never holds a viewing share, a PXE or a vault secret.

The app implements `DeviceStore`: a few small files and two 32-byte sealing keys from the Keychain or the Android
Keystore. Everything else is `OmeDevice`; `Review` describes what the member is asked to sign.

## Building the bindings

```sh
pnpm --filter @ome-labs/ffi mobile:swift     # Swift bindings on the macOS host: swift test
pnpm --filter @ome-labs/ffi mobile:kotlin    # Kotlin bindings on the JVM: gradle test
pnpm --filter @ome-labs/ffi mobile:apple     # dist/apple/OmeFfi.xcframework
pnpm --filter @ome-labs/ffi mobile:android   # dist/android/ome-ffi.aar
```

## Testing

```sh
cargo test --manifest-path crates/Cargo.toml -p ome-ffi
```

## Learn more

[architecture/mobile.md](https://github.com/ome-labs/ome/blob/main/architecture/mobile.md) describes the design. Ome: <https://ome.network>, <https://github.com/ome-labs/ome>.

## License

Apache-2.0. Not published to crates.io; built from this repository.
