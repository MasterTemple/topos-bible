/// <reference lib="webworker" />
// Runs in a Web Worker: searches files with its own copy of the engine, off the main thread
import init, { Topos } from "topos-bible";
import { searchText, type Hit } from "../core/search.ts";

export type WorkerRequest =
  | { type: "init"; wasm: Uint8Array }
  | { type: "search"; id: number; files: { path: string; text: string }[] };

export type WorkerResponse =
  | { type: "ready" }
  | { type: "results"; id: number; results: { path: string; hits: Hit[] }[] }
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
      scope.postMessage(await handle(event.data));
    } catch (error) {
      const id = event.data.type === "search" ? event.data.id : undefined;
      scope.postMessage({ type: "error", id, message: String(error) } satisfies WorkerResponse);
    }
  };
}
