import esbuild from "esbuild";
import { builtinModules } from "node:module";
import { fileURLToPath } from "node:url";

const production = process.argv[2] === "production";
const pkg = fileURLToPath(new URL("./node_modules/topos-bible/", import.meta.url));

const alias = {
  "topos-bible": `${pkg}topos_bible.js`,
  "topos-bible-wasm": `${pkg}topos_bible_bg.wasm`,
};

/**
 * Bundles the Web Worker on each build and exposes its code as `import source from
 * "topos-worker-source"`, so main.js stays a single file. The worker gets the WebAssembly from
 * the plugin at startup, so it is not embedded twice.
 */
const workerSource = {
  name: "worker-source",
  setup(build) {
    build.onResolve({ filter: /^topos-worker-source$/ }, (args) => ({ path: args.path, namespace: "worker" }));
    build.onLoad({ filter: /.*/, namespace: "worker" }, async () => {
      const result = await esbuild.build({
        entryPoints: ["src/indexers/worker.ts"],
        bundle: true,
        write: false,
        format: "iife",
        target: "es2022",
        minify: production,
        alias,
      });
      return { contents: result.outputFiles[0].text, loader: "text", watchFiles: ["src/indexers/worker.ts", "src/core/search.ts"] };
    });
  },
};

const context = await esbuild.context({
  entryPoints: ["src/main.ts"],
  plugins: [workerSource],
  bundle: true,
  // Obsidian and CodeMirror are provided by the app
  external: [
    "obsidian",
    "electron",
    "@codemirror/*",
    "@lezer/*",
    ...builtinModules,
    ...builtinModules.map((name) => `node:${name}`),
  ],
  // The package's core module (no automatic loading) and its WebAssembly, embedded in main.js
  alias,
  loader: { ".wasm": "binary" },
  format: "cjs",
  target: "es2022",
  jsx: "automatic",
  define: { "process.env.NODE_ENV": JSON.stringify(production ? "production" : "development") },
  logLevel: "info",
  sourcemap: production ? false : "inline",
  minify: production,
  treeShaking: true,
  outfile: "main.js",
});

if (production) {
  await context.rebuild();
  await context.dispose();
} else {
  await context.watch();
}
