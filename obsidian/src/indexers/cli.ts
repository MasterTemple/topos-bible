import { CliOutputParser, OutdatedCliError } from "../core/cli.ts";

/** A running `topos` search that can be stopped */
export interface CliRun {
  done: Promise<void>;
  stop(): void;
  /** The lines it printed to stderr (like `topos: <path>: <error>` for files it couldn't read) */
  errors: string[];
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
 * Runs `topos -m index` on the vault `paths` given (named, so ignore files can't skip them;
 * desktop only), and reports each file's index entry as it streams in. Exit code 1 only means
 * nothing was found.
 */
export function runCli(
  cliPath: string,
  vaultPath: string,
  { cache, extensions = [], paths }: { cache: boolean; extensions?: string[]; paths: string[] },
  onEntry: (path: string, bytes: Uint8Array) => void,
): CliRun {
  const { spawn } = require("node:child_process") as typeof import("node:child_process");
  const args = [
    "--no-config",
    "-m",
    "index",
    ...(cache ? ["--cache"] : []),
    ...(extensions.length > 0 ? ["--ext", extensions.join(",")] : []),
    "--",
    ...paths,
  ];
  const child = spawn(cliPath, args, { cwd: vaultPath, stdio: ["ignore", "pipe", "pipe"] });
  const parser = new CliOutputParser(onEntry);
  let stderr = "";
  const errors: string[] = [];
  let partial = "";
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
    child.stderr.on("data", (chunk: Buffer) => {
      const text = chunk.toString();
      if (stderr.length < 10_000) stderr += text;
      const lines = (partial + text).split("\n");
      partial = lines.pop() ?? "";
      errors.push(...lines.filter(Boolean));
    });
    child.on("error", reject);
    child.on("close", (code) => {
      if (partial) errors.push(partial);
      try {
        parser.finish();
      } catch (error) {
        return reject(error);
      }
      // 1 means nothing matched; 2 can still mean results with some unreadable files, unless the
      // arguments were rejected (an older topos without --ext)
      const badArguments = code === 2 && /^error:/m.test(stderr);
      if (code === 0 || code === 1 || (code === 2 && !badArguments)) resolve();
      else if (/invalid value 'index'/.test(stderr)) reject(new OutdatedCliError());
      else reject(new Error(stderr.trim() || `topos exited with code ${code}`));
    });
  });
  return { done, stop: () => child.kill(), errors };
}
