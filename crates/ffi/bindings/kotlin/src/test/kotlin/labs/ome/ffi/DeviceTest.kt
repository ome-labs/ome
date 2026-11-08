// The Kotlin bindings load the library and cross the boundary both ways: a store implemented in Kotlin, records,
// errors and suspend functions. The pairing, review and signing flows are tested in Rust (crates/ffi/tests/device.rs).
package labs.ome.ffi

import kotlinx.coroutines.runBlocking
import java.util.concurrent.ConcurrentHashMap
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** The app's side: files in memory and fixed sealing keys. */
class MemoryStore : DeviceStore {
    private val files = ConcurrentHashMap<String, ByteArray>()

    override fun read(name: String): ByteArray? = files[name]

    override fun write(name: String, bytes: ByteArray) {
        files[name] = bytes
    }

    override fun remove(name: String) {
        files.remove(name)
    }

    override fun sealingKey(purpose: KeyPurpose): ByteArray =
        when (purpose) {
            KeyPurpose.TRANSPORT -> ByteArray(32) { 1 }
            KeyPurpose.SIGNING -> ByteArray(32) { 2 }
        }
}

class DeviceTest {
    @Test
    fun aDeviceIsCreatedAndReopenedFromTheAppsStore() {
        val store = MemoryStore()
        val info = OmeDevice.create(store, "phone").info()
        assertEquals("phone", info.name)
        assertTrue(info.signerKey.x.startsWith("0x"))
        assertTrue(info.peerId.startsWith("12D3KooW"))

        val reopened = OmeDevice.open(store)
        assertEquals(info, reopened.info())
        assertNull(reopened.pairing())
        assertTrue(reopened.reviews().isEmpty())
    }

    @Test
    fun thePolicyRoundTripsAndBadValuesAreRefused() {
        val device = OmeDevice.create(MemoryStore(), "phone")
        val limit = TokenLimit("0x1001", "40")
        device.setPolicy(Policy(listOf(), listOf(limit), true, listOf()))
        assertEquals(listOf(limit), device.policy().tokenLimits)
        assertFailsWith<FfiException> {
            device.setPolicy(Policy(listOf("not hex"), listOf(), false, listOf()))
        }
    }

    @Test
    fun errorsAndSuspendFunctionsCrossTheBoundary() = runBlocking {
        val device = OmeDevice.create(MemoryStore(), "phone")
        assertFailsWith<FfiException> { parsePairOffer("ome-pair-v0:nope") }
        assertFailsWith<FfiException> { device.sync() }
        assertFailsWith<FfiException> { device.pair("nope", 1u) }
        Unit
    }
}
