/// <reference lib="webworker" />
// Runs in a Web Worker: searches files with its own copy of the engine, off the main thread
import init, { OffsetUnit, Topos } from "topos-bible";
import { searchText, type Hit } from "../core/search.ts";

export type WorkerRequest =
  | { type: "init"; wasm: Uint8Array }
  | { type: "search"; id: number; files: { path: string; text: string }[] }
  | { type: "index"; id: number; files: IndexRequest[] };

/** A file to search for the index */
export interface IndexRequest {
  path: string;
  size: number;
  mtime: number;
  text: string;
}

export type WorkerResponse =
  | { type: "ready" }
  | { type: "results"; id: number; results: { path: string; hits: Hit[] }[] }
  | { type: "entries"; id: number; entries: { path: string; bytes: Uint8Array }[] }
  | { type: "error"; id?: number; message: string };

let topos: Topos | null = null;

/** Handles one message (exported for tests) */
export async function handle(request: WorkerRequest, engine?: Topos): Promise<WorkerResponse> {
  if (request.type === "init") {
    await init(request.wasm);
    topos = Topos.new();
    return { type: "ready" };
  }
  const searcher = engine ?? topos;
  if (!searcher) return { type: "error", id: request.id, message: "the engine is not loaded" };
  if (request.type === "index") {
    // Entries in the index's format, so the hits never become objects
    const written = Date.now();
    const entries = request.files.map((file) => ({
      path: file.path,
      bytes: searcher.indexEntry(file.path, file.size, file.mtime, file.text, written, OffsetUnit.Utf16),
    }));
    return { type: "entries", id: request.id, entries };
  }
  const results = request.files.map((file) => ({
    path: file.path,
    hits: searchText(searcher, file.path, file.text),
  }));
  return { type: "results", id: request.id, results };
}

const scope = globalThis as unknown as DedicatedWorkerGlobalScope;
if (typeof scope.postMessage === "function" && typeof (globalThis as { document?: unknown }).document === "undefined") {
  scope.onmessage = async (event: MessageEvent<WorkerRequest>) => {
    try {
      const response = await handle(event.data);
      // The entries' buffers move to the main thread rather than being copied
      const transfer = response.type === "entries" ? response.entries.map((e) => e.bytes.buffer as ArrayBuffer) : [];
      scope.postMessage(response, transfer);
    } catch (error) {
      const id = event.data.type === "init" ? undefined : event.data.id;
      scope.postMessage({ type: "error", id, message: String(error) } satisfies WorkerResponse);
    }
  };
}
