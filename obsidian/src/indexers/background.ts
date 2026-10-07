import type { Hit } from "../core/search.ts";
import type { BookRequest, BookResult, IndexRequest, WorkerRequest, WorkerResponse } from "./worker.ts";

/** Searches files in a Web Worker, so indexing never blocks Obsidian */
export class BackgroundSearcher {
  private readonly worker: Worker;
  private readonly ready: Promise<void>;
  private nextId = 0;
  private readonly pending = new Map<number, { resolve: (value: never) => void; reject: (error: Error) => void }>();

  /** `source` is the worker's bundled code; `wasm` is the engine, copied into the worker */
  constructor(source: string, wasm: Uint8Array) {
    const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    this.worker = new Worker(url);
    URL.revokeObjectURL(url);
    this.ready = new Promise((resolve, reject) => {
      this.worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
        const message = event.data;
        if (message.type === "ready") resolve();
        else if (message.type === "results" || message.type === "entries" || message.type === "book") {
          const value = message.type === "results" ? message.results : message.type === "entries" ? message.entries : message.result;
          this.pending.get(message.id)?.resolve(value as never);
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

  private post(request: WorkerRequest, transfer: Transferable[] = []): void {
    this.worker.postMessage(request, transfer);
  }

  async search(files: { path: string; text: string }[]): Promise<{ path: string; hits: Hit[] }[]> {
    await this.ready;
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: never) => void, reject });
      this.post({ type: "search", id, files });
    });
  }

  /** Searches files into index entries (for `ToposIndex.insert`) */
  async index(files: IndexRequest[]): Promise<{ path: string; bytes: Uint8Array }[]> {
    await this.ready;
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: never) => void, reject });
      this.post({ type: "index", id, files });
    });
  }

  /** Searches an EPUB into its index entry (its bytes move to the worker) */
  async indexBook(book: BookRequest): Promise<BookResult> {
    await this.ready;
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: never) => void, reject });
      this.post({ type: "book", id, book }, [book.bytes]);
    });
  }

  terminate(): void {
    this.worker.terminate();
    for (const { reject } of this.pending.values()) reject(new Error("stopped"));
    this.pending.clear();
  }
}
