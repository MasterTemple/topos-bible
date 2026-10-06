import { CliOutputParser } from "../core/cli.ts";
import type { Hit } from "../core/search.ts";

/** A running `topos` search that can be stopped */
export interface CliRun {
  done: Promise<void>;
  stop(): void;
}

/** Where `topos` usually is when it was installed with cargo (GUI apps often lack ~/.cargo/bin in PATH) */
export function defaultCliPath(): string {
  // Only called on desktop, where Node's modules are available
  const { existsSync } = require("node:fs") as typeof import("node:fs");
  const { homedir } = require("node:os") as typeof import("node:os");
  const { join } = require("node:path") as typeof import("node:path");
  const exe = process.platform === "win32" ? "topos.exe" : "topos";
  const cargo = join(homedir(), ".cargo", "bin", exe);
  return existsSync(cargo) ? cargo : exe;
}

/**
 * Runs `topos -m json` over the vault folder (desktop only) and reports each file's hits as
 * they stream in. Exit code 1 only means nothing was found.
 */
export function runCli(
  cliPath: string,
  vaultPath: string,
  { cache }: { cache: boolean },
  onFile: (path: string, hits: Hit[]) => void,
): CliRun {
  const { spawn } = require("node:child_process") as typeof import("node:child_process");
  const args = [".", "--no-config", "-m", "json", ...(cache ? ["--cache"] : [])];
  const child = spawn(cliPath, args, { cwd: vaultPath, stdio: ["ignore", "pipe", "pipe"] });
  const parser = new CliOutputParser(onFile);
  let stderr = "";
  const done = new Promise<void>((resolve, reject) => {
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      try {
        parser.push(chunk);
      } catch (error) {
        child.kill();
        reject(error);
      }
    });
    child.stderr.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
    child.on("error", reject);
    child.on("close", (code) => {
      try {
        parser.finish();
      } catch (error) {
        return reject(error);
      }
      // 1 means nothing matched; 2 can still mean results with some unreadable files
      if (code === 0 || code === 1 || code === 2) resolve();
      else reject(new Error(stderr.trim() || `topos exited with code ${code}`));
    });
  });
  return { done, stop: () => child.kill() };
}
