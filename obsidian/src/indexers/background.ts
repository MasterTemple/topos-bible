import type { Hit } from "../core/search.ts";
import type { WorkerRequest, WorkerResponse } from "./worker.ts";

/** Searches files in a Web Worker, so indexing never blocks Obsidian */
export class BackgroundSearcher {
  private readonly worker: Worker;
  private readonly ready: Promise<void>;
  private nextId = 0;
  private readonly pending = new Map<
    number,
    { resolve: (results: { path: string; hits: Hit[] }[]) => void; reject: (error: Error) => void }
  >();

  /** `source` is the worker's bundled code; `wasm` is the engine, copied into the worker */
  constructor(source: string, wasm: Uint8Array) {
    const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    this.worker = new Worker(url);
    URL.revokeObjectURL(url);
    this.ready = new Promise((resolve, reject) => {
      this.worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
        const message = event.data;
        if (message.type === "ready") resolve();
        else if (message.type === "results") {
          this.pending.get(message.id)?.resolve(message.results);
          this.pending.delete(message.id);
        } else if (message.type === "error") {
          const error = new Error(message.message);
          if (message.id === undefined) reject(error);
          else {
            this.pending.get(message.id)?.reject(error);
            this.pending.delete(message.id);
          }
        }
      };
      this.worker.onerror = (event) => reject(new Error(event.message));
    });
    this.post({ type: "init", wasm: wasm.slice() });
  }

  private post(request: WorkerRequest): void {
    this.worker.postMessage(request);
  }

  async search(files: { path: string; text: string }[]): Promise<{ path: string; hits: Hit[] }[]> {
    await this.ready;
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.post({ type: "search", id, files });
    });
  }

  terminate(): void {
    this.worker.terminate();
    for (const { reject } of this.pending.values()) reject(new Error("stopped"));
    this.pending.clear();
  }
}
