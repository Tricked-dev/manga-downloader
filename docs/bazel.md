# Bazel Conversion

This repository is being migrated to Bazel in parallel with the existing Nix
and Cargo/PNPM build paths.

## Tooling

- `nix develop` provides Bazelisk, Aspect CLI, and Buildifier.
- `.bazelversion` installs the latest observed BuildBuddy CLI marker,
  `buildbuddy-io/5.0.373`, and then uses Bazelisk `last_green` so the underlying
  Bazel binary comes from a recent `bazelbuild/bazel` green build without
  compiling Bazel itself from source.
- The root `.bazelrc` is only a loader. Checked-in flags are split into flat
  fragment files under `tools/bazelrc/`.
- `.aspect/version.axl` pins Aspect CLI to `2026.22.44`.

The current `bazelbuild/bazel` `HEAD` observed on 2026-06-02 is
`9f648bde36dd2048524aee522ddcade56dd4dbb6`, which is also the live
`last_green` build label resolved by `nix develop -c bazel version`. We
intentionally use `last_green`, not that raw commit, because Bazelisk may build
arbitrary commit hashes from source when a prebuilt green binary is unavailable.

## App and Library Packages

Application and domain-library BUILD files are policy-checked through
`//libs/bazel/bazelrc:app_library_build_policy`. The repository keeps runnable
products under `//apps/...` and reusable code or build tooling under `//libs/...`:

- `//apps/clients/...`, `//apps/rust/...`, and `//apps/svelte/...` own
  binaries, deployable frontend builds, generated client artifacts, and
  app-private implementation code.
- `//libs/rust/...` and `//libs/svelte/...` own reusable runtime/domain/UI
  libraries that apps depend on.
- `//libs/rust/api-types` owns the derive-heavy API response and schema types so
  route and service code can depend on cached DTO artifacts instead of compiling
  those derives inside the server binary crate.
- `//libs/rust/page-extraction` owns the downloaded archive page extraction
  scheduler and archive identity schema, keeping that sizeable state machine out
  of the server binary crate so Bazel can cache and test it independently.
- `//libs/bazel/...` owns repository-local Starlark, policy, and build-tooling
  rules.

Each package family exposes `:apps`, `:libs`, `:tests`, and `:starlark_files`
aggregates where applicable. Root-level formatter and policy targets consume
those package-family aggregates instead of manually listing every leaf package,
so adding a new app or library should require updating only the nearest owning
aggregate. Expensive packaging artifacts, such as UPX-compressed binaries, stay
as explicit leaf targets rather than becoming part of the normal app aggregate.

## Tracked Prebuilt Exceptions

The migration prefers source-built tools where they materially affect build
outputs, cache keys, or compiler behavior. The current explicit exceptions are:

- Bazel itself is resolved by Bazelisk through `buildbuddy-io/5.0.373` and
  `last_green`. This keeps the repo on recent Bazel features without requiring
  local or CI builds of Bazel from source.
- Buildifier uses `buildifier_prebuilt` 8.5.1.2. A source-built
  `bazelbuild/buildtools` override was tested, but its module graph conflicts
  with the current `aspect_gazelle_js` Gazelle extension imports.
- Node.js uses checksum-pinned `node_repositories` archives for the frontend
  toolchain. Building Node from source is lower priority than keeping the
  SvelteKit targets hermetic and cacheable under Bazel.
- The apko CLI uses the pinned `v1.2.11` `rules_apko` toolchain while the RBE
  image contents themselves come from the locked Wolfi package graph.
- The RBE image push path uses `rules_img` to convert the apko OCI layout and
  push it to GHCR, avoiding the older `rules_oci` launcher-tool path.

`//libs/bazel/bazelrc:prebuilt_tool_policy` is part of `repo policy` and fails
if these tracked exceptions or their pins disappear from the checked-in config
and docs. Source-built LLVM and source-built `aspect_gazelle_js` stay policy
requirements because they are higher-impact compiler/parser paths.

## BuildBuddy

All Bazel builds use BuildBuddy remote cache and RBE by default:

```bash
nix develop -c bazel login
nix develop -c bazel build //...
```

This is intentionally not hidden behind a named config. If Bazel is the entry
point, it should use remote cache and RBE.
Remote cache compression is enabled with `--remote_cache_compression` alongside
BuildBuddy's content-defined chunking flag, so large action blobs can use both
wire compression and chunk-level cache reuse. A repo-local disk cache at
`.cache/bazel/disk` is also enabled with a one-day, 50 GiB GC bound so Bazel can
reuse downloaded chunks locally without sharing mutable cache state across
unrelated checkouts.
Remote cache lease extension is enabled so long remote builds periodically
refresh referenced output leases instead of relying on a single early cache
reference to remain valid for the whole invocation. The repo does not set an
oversized `--experimental_remote_cache_ttl`; that value should track the real
remote cache retention policy, and BuildBuddy Cloud does not expose a per-repo
TTL contract in this checkout.
Remote download hash verification is explicit, local fallback from failed remote
execution is disabled, and Bazel's concurrent input change guard runs in `full`
mode. Those defaults bias toward deterministic cache writes: if the remote
executor or cache path is unhealthy, the build should fail visibly rather than
silently producing or uploading local action results.
Remote action concurrency is set to `--jobs=200` so local invocations can keep
the remote executor saturated without adding a per-config override.
The default explicitly sets `--remote_download_minimal`, so Bazel downloads
top-level outputs on demand instead of materializing every remote action output
locally. This matches BuildBuddy's remote-runner guidance for smaller local
snapshots while keeping remote cache writes and BES uploads enabled.
BuildBuddy's remote-runner docs also mention Bazel's remote repository contents
cache for Bazel 9.1+ builds, but the current BuildBuddy CLI `last_green` Bazel
binary rejects `--experimental_remote_repo_contents_cache`, so that flag stays
out of the checked-in defaults until the selected binary exposes it.
Bzlmod lockfile handling defaults to `--lockfile_mode=error`, so normal builds,
tests, runs, and coverage fail instead of silently rewriting
`MODULE.bazel.lock`. After changing `MODULE.bazel` or a module extension input,
use `nix develop -c aspect repo mod-deps-update` to refresh the lockfile
explicitly.

BuildBuddy CLI stores the selected API key in local git config, not in this
repository. `BUILDBUDDY_API_KEY` still works for non-interactive environments.

Remote builds use separate repo-owned target and execution platforms. The target
platform, `//infra/images/rbe:platform_linux_x86_64`, selects the BCR `llvm`
module's Linux x86_64 musl constraints. The execution platform,
`//infra/images/rbe:execution_platform_linux_x86_64_ghcr`, keeps Rust build tools
and proc macros on the Linux glibc host ABI required by the Rust compiler and
points BuildBuddy at the apko-built GHCR executor image. The non-container
`//infra/images/rbe:execution_platform_linux_x86_64` platform is also registered
for bootstrap-only actions that explicitly require
`//infra/images/rbe:executor_bootstrap`.

The shared `common` defaults intentionally target Linux x86_64 for normal
build/test analysis and execution. The `run` command keeps only its host-platform
override command-scoped so developer-facing `bazel run` and `aspect repo start`
commands produce a local Darwin binary on macOS while still using the same
remote cache, BES, and execution defaults where Bazel can apply them.
The macOS deployment floor is pinned to `26.0` through both
`--macos_minimum_os` and `--host_macos_minimum_os`; that is the newest valid
deployment target in the currently selected local macOS 26.0 SDK, even though
the host OS patch level is newer. The Linux RBE platforms pin hermetic-LLVM's
libc constraint to `@llvm//constraints/libc:gnu.2.43`, matching the newest glibc
constraint available in the current hermetic-LLVM release and the repo-owned
executor image's glibc line.

The repo default sets `@llvm//toolchain:source=bootstrapped`, so C/C++ actions
use the source-built LLVM toolchain rather than BuildBuddy's Ubuntu toolchain.
Rust proc-macro links also enable `@llvm//config:experimental_stub_libgcc_s`, and
the LLVM module patch adds a tiny hermetic Linux sysroot containing
`libgcc_s.so` linker-script stubs under `lib/` and `usr/lib/`. That satisfies
rustc's early `-lgcc_s` probe before later source-built runtime search paths are
appended, without using executor GCC runtime packages or a linker wrapper.
The `//libs/bazel/bazelrc:*_policy` targets are wired into `repo doctor` and
`repo quality`. They fail if the BuildBuddy toolchain label is reintroduced, if
the source-built LLVM defaults disappear, if the BuildBuddy remote cache,
remote executor, BES, instance namespace, or RBE platform defaults disappear or
move back to build-only rc entries, if
BuildBuddy cache compression or content-defined chunking is removed, if minimal
remote downloads are replaced by full/toplevel downloads, if remote cache lease
extension is disabled, if the global Bzlmod lockfile error mode is removed, if
remote download verification or concurrent input change guarding is disabled, if
remote execution is allowed to fall back to local execution, if local or
standalone strategy flags are added to the default build, if compatibility
overrides for checked-in `--incompatible_*` flags reappear, if the split
`tools/bazelrc/` loader disappears, or if the source-built Aspect Gazelle JS/TS
setup is replaced by the prebuilt runner. They also fail if BuildBuddy workflows
gain pull-request triggers or if checked-in GitHub Actions workflows are
reintroduced.
The same package also registers a tiny default test toolchain wrapper so
`bazel test` and `bazel coverage` resolve on the Linux RBE target platform.
Rust builds use `rules_rs` with the registered source-built LLVM toolchain,
`@llvm//config:experimental_stub_libgcc_s=True`, and the RBE platform's
`@llvm//constraints/pie:off` constraint. The previous `rules_rust.patch`
extension hook has been removed; the current `//apps/rust/server:server` link
passes without the local rules_rust LIBRARY_PATH/link-input patches.

The generated `bazelrc-preset.bzl` file and
`tools/bazelrc/20-incompatible.bazelrc` keep the accepted boolean
`--incompatible_*` defaults enabled for the selected Bazel
`10.0.0-pre-d62d21b378c23e86692bd249161c29b15d33bb4d` binary. The list was
checked against the Bazel command-line reference and the matching
`bazelbuild/bazel` source commit, then validated with
`bazel build --nobuild //:gazelle_bin`. Flags that Bazel accepts but current
external rules cannot yet handle remain commented in that fragment with their
failing dependency and migration error.

The Go SDK is downloaded as Go 1.26.3 but bootstraps compiler binaries from
source with `//third_party/patches:go_compiler_flags.patch`, following
hermetic-LLVM's cgo compatibility guidance for Go versions before 1.27. That
lets cgo and Go linker probes consume the hermetic LLVM cc toolchain under RBE
and strict action env without checked-in host compiler overrides such as
`CC=clang` or `/usr/sbin/clang` linkopts.

The checked-in default currently uses the digest-pinned repo-owned executor
image
`ghcr.io/cyclopsbot/manga-downloader-rbe@sha256:a282ec63af83721aa65410c032bd7e77ede0e062c6bf9800e655efbf1dfbf08c`.
The repo-owned image is built and published at
`ghcr.io/cyclopsbot/manga-downloader-rbe@sha256:a282ec63af83721aa65410c032bd7e77ede0e062c6bf9800e655efbf1dfbf08c`,
and the GHCR package is public so BuildBuddy can pull it without registry
headers. If the GHCR package is made private again, pass registry credentials
through BuildBuddy's documented remote execution headers rather than platform
`exec_properties`:

```bazelrc
build --remote_exec_header=x-buildbuddy-platform.container-registry-username=<github-user>
build --remote_exec_header=x-buildbuddy-platform.container-registry-password=<ghcr-token>
```

`user.bazelrc` is ignored by git and already imported by `.bazelrc`, so it is the
local place for those headers. `user.bazelrc.example` is the checked-in template
for that local-only credentials file. BuildBuddy Workflows create `user.bazelrc`
from `GHCR_USERNAME` and `GHCR_TOKEN` secrets when they are present.

Coverage canaries require the executor image to provide `/usr/bin/env`, Bash,
Node's runtime libraries, and a glibc new enough for the source-built
LLVM/rules_rs host tools. The default image is the repo-owned apko image because
the older monorepo image failed Bazel shell/XML-wrapper paths, generic public
images missed pieces needed by either Node actions or the Rust toolchain, and
BuildBuddy's non-container fallback has an older glibc.
`repo coverage-repo-owned` runs coverage with the repo-owned image as both the
host and extra execution platform, and the BuildBuddy coverage workflow uses
that task after writing GHCR remote-execution headers from secrets.
`repo rbe-owned-image-check` is the no-cache canary for the same migration: it
builds a tiny action on the separate repo-owned GHCR execution platform and
should fail with a GHCR authorization error until the package is public or
BuildBuddy has registry headers.

BuildBuddy Workflows are configured in `buildbuddy.yaml`; the repo still needs
to be linked/enabled in the BuildBuddy UI before those checks run. The checked-in
workflow only runs for pushes to `main`, not pull requests. The quality workflow
runs formatter/lint targets, Bazel-built cargo-deny and cargo-shear checks,
Criterion benchmark compilation, and the backend SBOM graph.
Workflow steps install Ubuntu's `nix-bin` package when needed and then call the
same `nix develop --accept-flake-config -c aspect repo ...` tasks used locally.
That keeps BuildBuddy CI on the repo-owned Nix dev shell while Bazel still
resolves through `.bazelversion` and the BuildBuddy CLI marker.
The `NixOS sysupdate deploy` workflow is the only BuildBuddy workflow that joins
the tailnet for the Mac-hosted Celler cache. When the BuildBuddy secret
`TAILSCALE_AUTHKEY` is present, that workflow uses Tailscale userspace
networking and configures Nix to prefer `http://100.68.106.29:8080/vps` before
the existing fleet and `cache.nixos.org` substituters. The cache public key is
`vps:kGtlomXvtREiMQDVIapf7Kj5NRWN17ty3FnUexb6Iuo=`.
The first workflow action runs `repo doctor`, which is intentionally cheap and
catches Bazel version, a no-cache default RBE image canary, Linux
target-platform, host run-platform, and RBE image analysis failures before the
heavier build/test actions spend remote execution time.
`//libs/bazel/bazelrc:buildbuddy_workflow_policy` enforces that this stays a
BuildBuddy push-only CI setup and that `.github/workflows/**` remains empty.

The cargo-deny and cargo-shear targets intentionally build and execute the
Bazel-built tool binaries with `--version` only. Both upstream tools shell out to
`cargo metadata` for full workspace analysis, and this minimal RBE image does
not provide a hermetic Bazel label for Cargo. The repo therefore keeps
`//libs/bazel/cargo:cargo_manifest_policy` as the hermetic centralized-manifest
gate, `//libs/bazel/cargo:sys_crate_policy` as the gate that native Rust
`-sys` crates use BCR modules where available, and
`//libs/bazel/cargo:cargo_tool_policy` as the gate that prevents these targets
from quietly regressing back to Bash-backed tests or host-PATH Cargo.
When a Bazel-provided Cargo executable is introduced, these targets can be
upgraded to full metadata checks by adding that executable as a declared tool.

The current native crate policy requires BCR-backed `zlib`, `lz4`, `zstd`,
`libgit2`, and `mimalloc` modules to stay declared and injected into
`rules_rs`. The `libz-sys`, `libgit2-sys`, `lz4-sys`, and `zstd-sys`
annotations keep their build scripts disabled and wire them to those modules.
`libsqlite3-sys` remains the explicit exception: `toasty-driver-sqlite` 0.7
hardcodes rusqlite's bundled path, so that build script still runs until the
upstream crate can be replaced or patched.
TLS uses `reqwest`'s `rustls-no-provider` feature plus the shared
`//libs/rust/tls` helper, which installs the process-wide Graviola provider
before repo-owned `reqwest` clients are built. The policy allows upstream tools
or compatibility code to keep their normal `ring` dependencies rather than
vendoring crates for every update; the invariant is that application TLS client
construction goes through Graviola-backed rustls.

## Aspect Tasks

Project tasks are loaded from `.aspect/project.axl`, with implementations split
across `.aspect/tasks/*.axl` and shared helpers in `.aspect/libs/*.axl`. They
all use the `repo` group and are thin wrappers around Bazel commands so the
same BuildBuddy RBE/cache defaults apply:

```bash
nix develop -c aspect repo quality
nix develop -c aspect repo policy
nix develop -c aspect repo cargo-policy
nix develop -c aspect repo cargo-tools
nix develop -c aspect repo canonical-flags
nix develop -c aspect repo canonical-flags -- --remote_download_minimal --noremote_local_fallback
nix develop -c aspect repo action-graph
nix develop -c aspect repo action-graph -- --output=text 'mnemonic("BazelPolicyCheck", //libs/bazel/bazelrc:remote_cache_policy)'
nix develop -c aspect repo configured-targets
nix develop -c aspect repo configured-targets -- --output=label_kind //libs/bazel/bazelrc:remote_cache_policy
nix develop -c aspect repo cache-probe
nix develop -c aspect repo exec-log
nix develop -c aspect repo grpc-log
nix develop -c aspect repo bb-print
nix develop -c aspect repo bb-print -- --grpc_log=.cache/bazel/remote-grpc.log
nix develop -c aspect repo bb-explain -- --old .cache/bazel/execution-log.compact --new <other-log-or-invocation>
nix develop -c aspect repo bb-view -- <invocation-id-or-url> --lines=200
nix develop -c aspect repo bb-ask
nix develop -c aspect repo profile
nix develop -c aspect repo flamegraph -- --help
nix develop -c aspect repo build-events
nix develop -c aspect repo coverage
nix develop -c aspect repo coverage-repo-owned
nix develop -c aspect repo rbe-default-image-check
nix develop -c aspect repo rbe-owned-image-check
nix develop -c aspect repo doctor
nix develop -c aspect repo gazelle-check
nix develop -c aspect repo gazelle-build
nix develop -c aspect repo gazelle -- -mode=diff
nix develop -c aspect repo mod-deps-check
nix develop -c aspect repo mod-deps-update
nix develop -c aspect repo release
nix develop -c aspect repo sanitizers
nix develop -c aspect repo optimized
nix develop -c aspect repo start
nix develop -c aspect repo start -- --cache-trace
nix develop -c aspect repo bench --build-only
nix develop -c aspect repo bb-clientd-build
nix develop -c aspect repo rbe-image
nix develop -c aspect repo rbe-push
nix develop -c aspect repo rbe-smoke
```

Use `nix develop -c aspect repo --help` for the current task list. `repo bench`
builds Criterion targets first and only runs them when `--build-only=false`.
`repo policy` builds the checked-in Bazel policy targets for LLVM source
toolchain selection, BuildBuddy cache compression/chunking, compatibility
overrides, minimal remote downloads, BuildBuddy push-only workflows,
source-built Aspect Gazelle, supply-chain/SBOM metadata, tracked prebuilt-tool
exceptions, centralized Cargo manifests, and Bazel-built cargo tool wiring.
It also checks the Aspect task surface itself, including doctor/repo-check
composition, quality policy inclusion, and simple AXL copy/paste hygiene.
`repo cargo-policy` runs only the Cargo manifest/tool policy gates, while
`repo cargo-tools` builds the
Bazel-managed cargo-deny and cargo-shear tool actions. `repo mod-deps-check`
verifies `MODULE.bazel.lock` without
modifying it, while `repo mod-deps-update` is the explicit lockfile refresh path
after Bzlmod dependency edits. `repo doctor` runs the non-mutating lockfile check
plus the policy checks and cheap checks for the selected Bazel version, a
no-cache default RBE image canary, Linux server target analysis, host-platform
dev launcher analysis, and bootstrap RBE image analysis.
Application and domain-library BUILD files are policy-checked so app/lib
packages do not pin exact dependency versions in BUILD attributes or depend on
version-stamped external labels; dependency versions stay centralized in
`MODULE.bazel`, lockfiles, and `third_party` manifests.
`repo canonical-flags` runs Bazel's `canonicalize-flags` command. With no
arguments it canonicalizes the repo's remote/cache policy flags; pass flags
after `--` to inspect how Bazel normalizes a custom list.
`repo format-check` builds `//:format_check` for Starlark, tests
`//:rustfmt_test`, and tests `//:format_oxc_check` for OXC-backed JS/TS and JSON
formatting. Rust formatting runs through the Rust target graph instead of a
repo-wide source file aggregate. Starlark formatting uses the pinned
`buildifier_prebuilt` 8.5.1.2 toolchain as a tracked prebuilt exception until
the source-built `buildtools` module can coexist with the Aspect Gazelle JS
dependency graph. `repo lint` tests `//:lint` for OXC-backed JS/TS linting and
builds `//:clippy_check`, which runs Clippy through the `rules_rust` per-target
aspect.
`repo action-graph` runs Bazel `aquery`. By default it prints the action graph
for the remote-cache policy genrule, including action keys, inputs, command
line, execution platform, and execution info. Pass `aquery` arguments after `--`
to inspect other targets or output formats.
`repo configured-targets` runs Bazel `cquery`. By default it prints the
configured target kind for the remote-cache policy target; pass `cquery`
arguments after `--` to inspect transitions, target patterns, and
platform-sensitive graph state.
`repo cache-probe` is an opt-in cache debugging task. It builds selected targets
with `--experimental_remote_require_cached` and `--noremote_upload_local_results`
so a miss fails instead of warming the cache. By default it checks the tiny
remote-cache policy target; pass explicit targets when debugging cache drift for
a larger graph.
`repo exec-log` captures Bazel's compact execution log at
`.cache/bazel/execution-log.compact`. Use it on a target that is unexpectedly
re-executing, then compare compact logs with BuildBuddy's `bb explain` or
Bazel's execlog tooling. Fully cached invocations may have little spawn detail
because no actions actually ran. The selected Bazel pre-release rejects the
older `--experimental_execution_log_spawn_metrics` flag, so the task uses the
portable compact-log flag only.
`repo grpc-log` captures Bazel's remote cache and remote execution gRPC traffic
at `.cache/bazel/remote-grpc.log` using the current `--remote_grpc_log` flag.
Use it when debugging remote-cache misses, executor traffic, upload/download
behavior, or bb_clientd forwarding; render the log with
`repo bb-print -- --grpc_log=.cache/bazel/remote-grpc.log`.
`repo bb-print` prints Bazel log files through BuildBuddy's log printer. By
default it renders `.cache/bazel/execution-log.compact` with sorting enabled,
so it pairs directly with `repo exec-log`; pass `bb print` arguments after `--`
to inspect another compact execution log or a remote gRPC log.
`repo bb-explain` forwards arguments to BuildBuddy's `bb explain`, which can
compare two compact execution-log file paths or two BuildBuddy invocation IDs.
With no arguments it compares the last two builds known to the local BuildBuddy
CLI.
`repo bb-view` forwards to `bb view` for terminal access to BuildBuddy
invocation logs, and `repo bb-ask` forwards to `bb ask` for BuildBuddy's
suggestions about the previous invocation. Both are intentionally diagnostic
wrappers and do not change Bazel's remote execution or cache defaults.
`repo profile` captures Bazel's JSON trace profile at
`.cache/bazel/profile.json.gz` with target labels and target configurations
included. The selected Bazel pre-release does not expose the older
`analyze-profile` command, so inspect this file with a Chrome trace-compatible
viewer or another Bazel profile analyzer.
`repo flamegraph` builds `//apps/rust/server:server` in Bazel release mode and
then runs it locally under the Bazel-built `flamegraph` tool. By default the
profile is written to `.cache/bazel/flamegraphs/server.svg`; set
`MANGA_FLAMEGRAPH_OUTPUT=/path/to/profile.svg` to choose another path and pass
server arguments after `--`. This is a local diagnostic run target because
sampling profiles need host kernel or macOS profiler access; on Linux,
`flamegraph` relies on `perf`, and on macOS it relies on the platform tracing
tools.
`repo build-events` captures a local Build Event Protocol JSON stream at
`.cache/bazel/build-events.json`. The task writes Bazel's raw stream to
`.cache/bazel/build-events.raw.json`, redacts BuildBuddy API headers and GHCR
registry password headers from expanded command-line events in AXL, writes the
sanitized file into the stable final path, and removes the raw file so raw
credentials are not retained locally. It enables all-action publishing, target
summary events, and full mnemonic metrics for the invocation so local
diagnostics have the same structured build/test event shape that BuildBuddy and
other BEP consumers use.
`repo start` runs the Bazel-built Rust dev launcher that supervises the backend
and frontend `bazel run` targets, and forwards arguments after `--` to
`//apps/rust/dev-start:start`. `repo k6` runs Bazel k6 launcher targets for a
separately running backend; those targets use a Bazel-built Rust runner and
include the shared k6 TypeScript support files in runfiles.
`repo optimized` mirrors the BuildBuddy optimized-mode workflow by building the
server with `--config=release` and running the ASan/LSan filesystem canaries.
`repo rbe-image` relies on the repo defaults plus the image target's
`//infra/images/rbe:executor_bootstrap` execution constraint, so it bootstraps
the apko image without requiring the GHCR image to be pullable first and without
passing a per-task execution-platform override.
`repo rbe-push` runs `//infra/images/rbe:push`, which publishes the apko image
to GHCR. The image is still built through Bazel's remote/cache defaults, while
the final registry launcher carries host-platform `crane` and `jq` runfiles so
`bazel run` works from the macOS developer workstation instead of trying to
execute Linux RBE tools locally.
`repo rbe-default-image-check` runs a canary action against the checked-in
default repo-owned execution platform with a fresh `RBE_CANARY` action
environment value, so it proves BuildBuddy can execute on the current default
image instead of only finding a cached result.
`repo rbe-owned-image-check` uses the same volatile action-key pattern with
`RBE_REPO_OWNED_CANARY` against
`//infra/images/rbe:execution_platform_linux_x86_64_repo_owned_ghcr`. Bazel's
`no-cache` tag disables local and remote cache stores for the action, but it
does not disable every persistent local action state, so the action environment
changes on every run when the task is meant to prove the executor actually used
the image. This canary remains useful even though the repo-owned image is now
the default, because it proves the separate repo-owned executor constraint and
registry authentication path still work.
`repo rbe-smoke` uses the checked-in default repo-owned execution platform and
exercises the empty Linux sysroot target, Bazel-built cargo tool actions,
policy targets, and apko RBE image analysis.
`repo-check` starts with `repo doctor`'s checks before the broader build, test,
coverage, quality, and image targets.
`repo gazelle-check` runs the same `//:gazelle` binary in `-mode=diff` so
generated BUILD drift is visible without modifying files. `repo quality` includes
that check before formatter, linter, Cargo policy, k6, and SBOM targets.
`repo bb-clientd-build` is an opt-in path for builds through a running
bb_clientd local proxy; it layers `--config=bb-clientd` on top of the same
BuildBuddy backend and defaults to `//apps/rust/server:server`.

Release and sanitizer modes are opt-in `.bazelrc` configs layered on top of the
same remote cache/RBE defaults. `--config=release` uses optimized Rust codegen
with stripped symbols and applies `-O3` to target and exec/host C/C++ builds, so
source-built C/C++ dependencies and packaging tools use the same optimization
policy. `--config=asan` and `--config=lsan` set `RUSTC_BOOTSTRAP=1` for the Rust
`-Zsanitizer` flag and are wired into BuildBuddy against the fast filesystem
test canary. ThinLTO was tested but is not checked in yet because the current
rules/toolchain combination requires embedded bitcode and produces very large
intermediate artifacts.

## Gazelle

`//:gazelle_bin` builds Gazelle with the default Go/proto languages,
`Calsign/gazelle_rust`, and Aspect's source-built JavaScript/TypeScript language
from `aspect-build/aspect-gazelle`. The JS/TS language is supplied by the
`aspect_gazelle_js` BCR module, which builds its oxc parser through `rules_rs`
and the hermetic `llvm` module instead of downloading the prebuilt
`aspect_gazelle_prebuilt` runner. `aspect_rules_ts` is also declared so generated
`ts_project` packages can be adopted as frontend BUILD files are moved from
manual `rules_js` targets to generated targets.

The root defaults keep JS generation disabled until a package opts in with
`# gazelle:js enabled`. This lets Rust Gazelle remain authoritative for backend
packages while allowing the Svelte packages to migrate incrementally without
rewriting the hand-tuned SvelteKit/Vite targets in one pass. The root
`# gazelle:js_pnpm_lockfile pnpm-lock.yaml` directive points the JS resolver at
the repo's PNPM lockfile when a package opts in.
The root module still uses `rules_rs`; Gazelle's Rust output is configured to
resolve external crate labels against `@backend_crates//:` and generated Rust
rule loads target the `@rules_rust` compatibility repository exported by
`rules_rs`, not a root `rules_rust` module dependency.

## bb_clientd

`libs/bazel/buildbarn/bb_clientd.buildbuddy.jsonnet` is a repo-owned bb_clientd
configuration for BuildBuddy. It routes the `remote.buildbuddy.io/*` instance
prefix to BuildBuddy, keeps local AC/CAS state under `.cache/bb_clientd`, mounts
lazy outputs under `.cache/bb_clientd/mount`, and forwards Bazel's BuildBuddy
credentials to the upstream gRPC calls.

Start bb_clientd from a checked-out `buildbarn/bb-clientd` source tree after
reviewing the config:

```bash
mkdir -p .cache/bb_clientd/ac/persistent_state .cache/bb_clientd/cas/persistent_state .cache/bb_clientd/outputs .cache/bb_clientd/mount
OS="$(uname)" XDG_CACHE_HOME="$PWD/.cache" bazel run //cmd/bb_clientd -- "$PWD/libs/bazel/buildbarn/bb_clientd.buildbuddy.jsonnet"
```

Then run a build through the daemon:

```bash
nix develop -c bazel build --config=bb-clientd //apps/rust/server:server
nix develop -c aspect repo bb-clientd-build
```

The default `.bazelrc` still talks directly to BuildBuddy. The bb_clientd config
only overrides remote cache/executor/output-service endpoints when
`--config=bb-clientd` is present.

## RBE Image

`//infra/images/rbe:image` builds the BuildBuddy executor image with
`rules_apko` from the locked Wolfi package graph in
`infra/images/rbe/apko.lock.json`. The root `//:apko_bazelrc` target generates
the `.apko/` range-helper files used by Bazel repository downloads.

The executor image intentionally avoids host compiler and libc development
packages. Bazel supplies C/C++ compilation through the hermetic LLVM module
toolchain, so the image only carries basic shell/archive/Python support for
remote actions.

The current Bazel pre-release does not honor `rules_apko`'s direct `Range`
headers for APK segment fetches, so `MODULE.bazel` applies
`//third_party/patches:rules_apko_range_header.patch` to force the documented
credential-helper URL-fragment path. Remove that patch after upstream
`rules_apko` or the selected Bazel binary handles ranged downloads correctly.

`//infra/images/rbe:push` publishes `ghcr.io/cyclopsbot/manga-downloader-rbe`.
The BuildBuddy workflow only attempts the push on `main` when `GHCR_USERNAME`
and `GHCR_TOKEN` are present as BuildBuddy secrets. The image build and push
path can be bootstrapped before the GHCR executor image is public without
passing a workflow-specific execution-platform flag. The push rule is patched to
allow explicit launcher tool overrides, and the repo pins the RBE image push
launcher to Darwin arm64 `crane` and `jq` targets so local `repo rbe-push` uses
host-runnable tools while preserving the remote-built image artifact.

Current verification: `//infra/images/rbe:image` builds successfully through the
remote cache/executor and produces the apko image with glibc 2.43. `bazel run
//infra/images/rbe:push` has been verified from macOS with the host launcher
runfiles and published
`ghcr.io/cyclopsbot/manga-downloader-rbe:latest@sha256:a282ec63af83721aa65410c032bd7e77ede0e062c6bf9800e655efbf1dfbf08c`.

## Supply Chain

`//:backend_sbom_graph` uses `bazel-contrib/supply-chain` to collect package
metadata for the Bazel backend graph. The released BCR module currently exposes
an SPDX rule that points at `@supply-chain-go//cmd/spdx`, but
`supply-chain-go 0.0.6` does not publish that Bazel package, so this migration
keeps the buildable SBOM graph target wired and leaves SPDX/CycloneDX emission
for a later supply-chain module release or an upstream override.
`//libs/bazel/bazelrc:supply_chain_policy` is part of `repo policy` and checks
that the `package_metadata` and `supply_chain_tools` modules, root package
metadata, backend SBOM graph, Aspect `repo sbom` task, and documented SPDX
limitation stay synchronized.
