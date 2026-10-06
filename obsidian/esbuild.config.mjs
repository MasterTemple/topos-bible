import esbuild from "esbuild";
import { builtinModules } from "node:module";
import { fileURLToPath } from "node:url";

const production = process.argv[2] === "production";
const pkg = fileURLToPath(new URL("./node_modules/topos-bible/", import.meta.url));

const context = await esbuild.context({
  entryPoints: ["src/main.ts"],
  bundle: true,
  // Obsidian and CodeMirror are provided by the app
  external: [
    "obsidian",
    "electron",
    "@codemirror/*",
    "@lezer/*",
    ...builtinModules,
  ],
  // The package's core module (no automatic loading) and its WebAssembly, embedded in main.js
  alias: {
    "topos-bible": `${pkg}topos_bible.js`,
    "topos-bible-wasm": `${pkg}topos_bible_bg.wasm`,
  },
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
