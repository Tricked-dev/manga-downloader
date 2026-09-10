#!/usr/bin/env node
import { spawnSync } from "node:child_process";

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    ...options,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function read(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });

  if (result.status !== 0) {
    return null;
  }

  const value = result.stdout.trim();
  return value.length > 0 ? value : null;
}

function sanitizeVarValue(value) {
  return value?.replace(/[\r\n]+/g, " ").trim() ?? null;
}

function readGitValue(args) {
  return sanitizeVarValue(read("git", args));
}

function readCurrentBranch() {
  const branch = readGitValue(["branch", "--show-current"]);
  if (branch) {
    return branch;
  }

  const ref = readGitValue(["rev-parse", "--abbrev-ref", "HEAD"]);
  return ref && ref !== "HEAD" ? ref : null;
}

function readUpstreamBranch() {
  const upstream = readGitValue(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
  return upstream === "HEAD" ? null : upstream;
}

function readGitClean() {
  const result = spawnSync("git", ["status", "--porcelain"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });

  if (result.status !== 0) {
    return null;
  }

  return result.stdout.trim().length === 0 ? "true" : "false";
}

function collectDeploymentVars() {
  const vars = {
    GIT_BRANCH: readCurrentBranch(),
    GIT_CLEAN: readGitClean(),
    GIT_COMMIT_AT: readGitValue(["show", "-s", "--format=%cI", "HEAD"]),
    GIT_COMMIT_MESSAGE: readGitValue(["show", "-s", "--format=%s", "HEAD"]),
    GIT_COMMIT_SHA: readGitValue(["rev-parse", "HEAD"]),
    GIT_COMMIT_SHORT_SHA: readGitValue(["rev-parse", "--short=7", "HEAD"]),
    GIT_REMOTE_BRANCH: readUpstreamBranch(),
    GIT_REMOTE_COMMIT_SHA: readGitValue(["rev-parse", "@{u}"]),
    GIT_REMOTE_COMMIT_SHORT_SHA: readGitValue(["rev-parse", "--short=7", "@{u}"]),
    PUBLIC_REPOSITORY_URL: readGitValue(["config", "--get", "remote.origin.url"]),
  };

  return Object.entries(vars)
    .filter((entry) => entry[1])
    .flatMap(([key, value]) => ["--var", `${key}:${value}`]);
}

const deployArgs = process.argv.slice(2);

run("pnpm", ["run", "build"]);
run("pnpm", ["exec", "wrangler", "deploy", ...collectDeploymentVars(), ...deployArgs]);
