import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

const [packageName, maybeBinName, ...rawArgs] = process.argv.slice(2);

if (!packageName || !maybeBinName) {
  console.error("usage: run-npm-bin.mjs <package> [bin] [args...]");
  process.exit(2);
}

const require = createRequire(pathToFileURL(join(process.cwd(), "package.json")));
const packageJsonPath = require.resolve(`${packageName}/package.json`);
const packageJson = require(packageJsonPath);
let binName = maybeBinName;
let args = rawArgs;
let bin = typeof packageJson.bin === "string" ? packageJson.bin : packageJson.bin?.[binName];

if (!bin && typeof packageJson.bin === "object" && Object.keys(packageJson.bin).length === 1) {
  binName = Object.keys(packageJson.bin)[0];
  bin = packageJson.bin[binName];
  args = [maybeBinName, ...rawArgs];
}

if (!bin) {
  console.error(`package ${packageName} does not declare bin ${binName}`);
  process.exit(2);
}

const result = spawnSync(process.execPath, [join(dirname(packageJsonPath), bin), ...args], {
  stdio: "inherit",
});

if (result.signal) {
  console.error(`${binName} exited with signal ${result.signal}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
