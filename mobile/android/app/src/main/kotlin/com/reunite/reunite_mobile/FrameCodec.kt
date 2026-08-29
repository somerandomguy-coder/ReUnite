package com.reunite.reunite_mobile

import java.io.ByteArrayOutputStream

/**
 * Length-prefixed framing over BLE writes.
 *
 * A mesh Frame is a few hundred bytes for a beacon and can reach kilobytes for an invite,
 * while a BLE characteristic write carries MTU-3 bytes — 20 by default, ~512 after
 * negotiation. So every frame goes out as a 4-byte little-endian length followed by its
 * bytes, split across as many writes as it takes, and the receiver reassembles per device.
 *
 * Per-device buffers matter: two peers writing at once would otherwise interleave into
 * one stream and corrupt both.
 */
object FrameCodec {
    /** Refuse anything larger than meshcore's own MAX_FRAME_BYTES, so a hostile or
     *  corrupt length cannot make us allocate without bound. */
    const val MAX_FRAME_BYTES = 8 * 1024

    fun encode(frame: ByteArray): ByteArray {
        val out = ByteArray(4 + frame.size)
        val n = frame.size
        out[0] = (n and 0xFF).toByte()
        out[1] = ((n shr 8) and 0xFF).toByte()
        out[2] = ((n shr 16) and 0xFF).toByte()
        out[3] = ((n shr 24) and 0xFF).toByte()
        frame.copyInto(out, 4)
        return out
    }

    fun chunk(payload: ByteArray, mtuPayload: Int): List<ByteArray> {
        val size = mtuPayload.coerceAtLeast(20)
        val chunks = ArrayList<ByteArray>((payload.size / size) + 1)
        var offset = 0
        while (offset < payload.size) {
            val end = minOf(offset + size, payload.size)
            chunks.add(payload.copyOfRange(offset, end))
            offset = end
        }
        return chunks
    }

    /** Accumulates writes from one device and emits whole frames. */
    class Reassembler {
        private val buffer = ByteArrayOutputStream()

        fun add(bytes: ByteArray): List<ByteArray> {
            buffer.write(bytes)
            val frames = ArrayList<ByteArray>()
            var data = buffer.toByteArray()
            var consumed = 0
            while (data.size - consumed >= 4) {
                val len = (data[consumed].toInt() and 0xFF) or
                    ((data[consumed + 1].toInt() and 0xFF) shl 8) or
                    ((data[consumed + 2].toInt() and 0xFF) shl 16) or
                    ((data[consumed + 3].toInt() and 0xFF) shl 24)
                if (len <= 0 || len > MAX_FRAME_BYTES) {
                    // Desynchronised or malicious: drop everything buffered for this
                    // device rather than trying to resync into arbitrary bytes.
                    buffer.reset()
                    return frames
                }
                if (data.size - consumed - 4 < len) break
                frames.add(data.copyOfRange(consumed + 4, consumed + 4 + len))
                consumed += 4 + len
            }
            if (consumed > 0) {
                val rest = data.copyOfRange(consumed, data.size)
                buffer.reset()
                buffer.write(rest)
                data = rest
            }
            return frames
        }

        fun reset() = buffer.reset()
    }
}
