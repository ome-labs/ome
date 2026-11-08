// swift-tools-version: 6.0
// Host smoke tests of the Swift bindings: `node scripts/mobile.ts swift` copies this package to dist/swift, adds the
// generated sources and links the macOS static library it built.
import Foundation
import PackageDescription

let library = Context.environment["OME_FFI_STATIC_LIB"] ?? "../../../target/release/libome_ffi.a"

let package = Package(
  name: "OmeFfi",
  platforms: [.macOS(.v14)],
  products: [.library(name: "OmeFfi", targets: ["OmeFfi"])],
  targets: [
    .target(name: "OmeFfiFFI", path: "Sources/OmeFfiFFI"),
    .target(
      name: "OmeFfi",
      dependencies: ["OmeFfiFFI"],
      path: "Sources/OmeFfi",
      linkerSettings: [
        .unsafeFlags([library]),
        .linkedFramework("CoreFoundation"),
        .linkedFramework("Security"),
        .linkedFramework("SystemConfiguration"),
      ]
    ),
    .testTarget(name: "OmeFfiTests", dependencies: ["OmeFfi"], path: "Tests/OmeFfiTests"),
  ]
)
