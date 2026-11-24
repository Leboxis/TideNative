import { Channel, invoke } from "@tauri-apps/api/core";

export interface DecodedTelemetryMeta {
  positionMs: number;
  durationMs: number;
  bufferedMs: number;
}

export function decodeTelemetryFrame(
  buffer: ArrayBufferLike,
  outLevels: Float32Array,
): DecodedTelemetryMeta | null {
  if (buffer.byteLength < 44) return null;
  const view = new DataView(buffer);
  for (let i = 0; i < 5; i++) {
    outLevels[i] = view.getFloat32(i * 4, true);
  }
  const positionMs = Number(view.getBigUint64(20, true));
  const durationMs = Number(view.getBigUint64(28, true));
  const bufferedMs = Number(view.getBigUint64(36, true));
  return { positionMs, durationMs, bufferedMs };
}

type TelemetrySubscriber = (meta: DecodedTelemetryMeta, levels: Float32Array) => void;

class TelemetryRuntime {
  readonly levels = new Float32Array(5);
  positionMs = 0;
  durationMs = 0;
  bufferedMs = 0;

  private subscribers = new Set<TelemetrySubscriber>();
  private started = false;

  subscribe(fn: TelemetrySubscriber): () => void {
    this.subscribers.add(fn);
    return () => {
      this.subscribers.delete(fn);
    };
  }

  seedPosition(positionMs: number, durationMs: number): void {
    this.positionMs = positionMs;
    this.durationMs = durationMs;
    const meta = {
      positionMs: this.positionMs,
      durationMs: this.durationMs,
      bufferedMs: this.bufferedMs,
    };
    for (const sub of this.subscribers) {
      sub(meta, this.levels);
    }
  }

  async start(): Promise<void> {
    if (this.started) return;
    this.started = true;

    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (payload) => {
      const buf =
        payload instanceof ArrayBuffer
          ? payload
          : ArrayBuffer.isView(payload)
            ? (payload as Uint8Array).buffer.slice(
                (payload as Uint8Array).byteOffset,
                (payload as Uint8Array).byteOffset + (payload as Uint8Array).byteLength,
              )
            : null;
      if (!buf) return;
      const meta = decodeTelemetryFrame(buf, this.levels);
      if (!meta) return;
      this.positionMs = meta.positionMs;
      this.durationMs = meta.durationMs;
      this.bufferedMs = meta.bufferedMs;
      for (const sub of this.subscribers) {
        sub(meta, this.levels);
      }
    };

    try {
      await invoke("subscribe_telemetry", { channel });
    } catch {
      this.started = false;
    }
  }
}

export const telemetryRuntime = new TelemetryRuntime();
