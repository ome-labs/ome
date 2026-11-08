// The Swift bindings load the library and cross the boundary both ways: a store implemented in Swift, records,
// errors and async methods. The pairing, review and signing flows are tested in Rust (crates/ffi/tests/device.rs).
import Foundation
import OmeFfi
import Testing

/// The app's side: files in memory and fixed sealing keys.
final class MemoryStore: DeviceStore, @unchecked Sendable {
  private let lock = NSLock()
  private var files: [String: Data] = [:]

  func read(name: String) throws -> Data? { lock.withLock { files[name] } }
  func write(name: String, bytes: Data) throws { lock.withLock { files[name] = bytes } }
  func remove(name: String) throws { _ = lock.withLock { files.removeValue(forKey: name) } }

  func sealingKey(purpose: KeyPurpose) throws -> Data {
    switch purpose {
    case .transport: Data(repeating: 1, count: 32)
    case .signing: Data(repeating: 2, count: 32)
    }
  }
}

@Test func aDeviceIsCreatedAndReopenedFromTheAppsStore() throws {
  let store = MemoryStore()
  let device = try OmeDevice.create(store: store, name: "phone")
  let info = device.info()
  #expect(info.name == "phone")
  #expect(info.signerKey.x.hasPrefix("0x"))
  #expect(info.peerId.hasPrefix("12D3KooW"))

  let reopened = try OmeDevice.open(store: store)
  #expect(reopened.info() == info)
  #expect(try reopened.pairing() == nil)
  #expect(try reopened.reviews().isEmpty)
}

@Test func thePolicyRoundTripsAndBadValuesAreRefused() throws {
  let device = try OmeDevice.create(store: MemoryStore(), name: "phone")
  let limit = TokenLimit(token: "0x1001", maxAmount: "40")
  try device.setPolicy(
    policy: Policy(allowedContracts: [], tokenLimits: [limit], requirePinnedLabels: true, pins: []))
  #expect(try device.policy().tokenLimits == [limit])
  #expect(throws: FfiError.self) {
    try device.setPolicy(
      policy: Policy(
        allowedContracts: ["not hex"], tokenLimits: [], requirePinnedLabels: false, pins: []))
  }
}

@Test func errorsAndAsyncMethodsCrossTheBoundary() async throws {
  let device = try OmeDevice.create(store: MemoryStore(), name: "phone")
  #expect(throws: FfiError.self) { try parsePairOffer(offer: "ome-pair-v0:nope") }
  await #expect(throws: FfiError.self) { try await device.sync() }
  await #expect(throws: FfiError.self) { try await device.pair(offer: "nope", waitSecs: 1) }
}
