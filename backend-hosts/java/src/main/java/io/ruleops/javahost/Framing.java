package io.ruleops.javahost;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Length-delimited frame transport over stdio (ADR-0030): 4-byte big-endian
 * payload length + payload bytes, mirroring the Rust erf-framing crate. The
 * frame limit is negotiated in the handshake; oversized frames are refused by
 * the peer, never truncated here.
 */
final class Framing {

    private final DataInputStream in;
    private final DataOutputStream out;
    private final long maxFrameBytes;

    Framing(InputStream in, OutputStream out, long maxFrameBytes) {
        this.in = new DataInputStream(in);
        this.out = new DataOutputStream(out);
        this.maxFrameBytes = maxFrameBytes;
    }

    /** Reads one frame; returns null on clean EOF at a frame boundary. */
    byte[] readFrame() throws IOException {
        int len;
        try {
            len = in.readInt();
        } catch (EOFException e) {
            return null;
        }
        if (len < 0 || len > maxFrameBytes) {
            throw new IOException("frame length " + len + " exceeds negotiated limit " + maxFrameBytes);
        }
        byte[] payload = new byte[len];
        in.readFully(payload);
        return payload;
    }

    void writeFrame(byte[] payload) throws IOException {
        if (payload.length > maxFrameBytes) {
            throw new IOException("payload exceeds negotiated frame limit");
        }
        out.writeInt(payload.length);
        out.write(payload);
        out.flush();
    }
}
