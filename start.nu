#!/usr/bin/env nu

const SCRIPT_DIR = (path self .)
const DEFAULT_RUST_LOG = "info,manga_server=info,chromiumoxide=error"
const CACHE_TRACE_RUST_LOG = "info,manga_server=trace,manga_server::cache=trace,chromiumoxide=error"
const DEFAULT_DB_PATH = "./data/manga.db"
const DEFAULT_DOWNLOAD_PATH = "./data/downloads"
const DEFAULT_PLUGINS_PATH = "./plugins"
const DEFAULT_SERVER_ADDR = "0.0.0.0:4000"

load-env {
    DB_PATH: ($env.DB_PATH? | default $DEFAULT_DB_PATH)
    DOWNLOAD_PATH: ($env.DOWNLOAD_PATH? | default $DEFAULT_DOWNLOAD_PATH)
    SERVER_ADDR: ($env.SERVER_ADDR? | default $DEFAULT_SERVER_ADDR)
    PLUGINS_PATH: ($env.PLUGINS_PATH? | default $DEFAULT_PLUGINS_PATH)
    SOURCE_PLUGIN_REGISTRY_URL: ($env.SOURCE_PLUGIN_REGISTRY_URL? | default "")
    DISCORD_BOT_TOKEN: ($env.DISCORD_BOT_TOKEN? | default "")
    DISCORD_CHANNEL_ID: ($env.DISCORD_CHANNEL_ID? | default "")
}

def repo-path [...parts: string] {
    [$SCRIPT_DIR ...$parts] | path join | path expand
}

def resolve-workspace-path [path: string] {
    if ($path | str starts-with "/") or ($path | str starts-with "~") {
        $path | path expand
    } else {
        repo-path $path
    }
}

def paint [color: string, value: any] {
    $"(ansi $color)($value)(ansi reset)"
}

def log [label: string, color: string, message: string] {
    print $"(paint $color $"[($label)]") ($message)"
}

def log-step [message: string] {
    log "step" cyan $message
}

def log-ok [message: string] {
    log "ok" green $message
}

def log-warn [message: string] {
    log "warn" yellow $message
}

def log-setting [name: string, value: any] {
    print $"  (paint cyan $name) = ($value)"
}

def db-sidecar-paths [db_path: string] {
    [$db_path $"($db_path)-shm" $"($db_path)-wal"]
}

def prepare-default-db-path [] {
    if $env.DB_PATH != $DEFAULT_DB_PATH {
        $env.DB_PATH = (resolve-workspace-path $env.DB_PATH)
        return
    }

    let default_db_path = (repo-path "data" "manga.db")
    let legacy_db_path = (repo-path "manga.db")

    $env.DB_PATH = $default_db_path

    if not ($legacy_db_path | path exists) {
        return
    }

    if ($default_db_path | path exists) {
        log-warn $"Found both legacy ($legacy_db_path) and current ($default_db_path) databases; leaving them untouched and using ($default_db_path)."
        return
    }

    mkdir ($default_db_path | path dirname)

    db-sidecar-paths $legacy_db_path
    | zip (db-sidecar-paths $default_db_path)
    | each {|pair|
        let source = ($pair | get 0)
        let dest = ($pair | get 1)

        if ($source | path exists) {
            mv $source $dest
        }
    }
    | ignore

    log-ok $"Moved legacy local database files into ($default_db_path)."
}

def ensure-nix [] {
    if (which nix | is-empty) {
        error make {
            msg: "Error: start.nu needs Nix to enter the Bazel launcher environment."
            help: "Install Nix, then run `nu ./start.nu` again."
        }
    }
}

def selected-rust-log [enable_cache_trace: bool] {
    if $enable_cache_trace {
        $CACHE_TRACE_RUST_LOG
    } else {
        $env.RUST_LOG? | default $DEFAULT_RUST_LOG
    }
}

def env-status [value: string, --secret] {
    let trimmed = ($value | str trim)
    if $trimmed == "" {
        paint yellow "not set"
    } else if $secret {
        paint green "******"
    } else {
        $trimmed
    }
}

def default-flamegraph-output [] {
    let stamp = (date now | format date "%Y%m%dT%H%M%S")
    repo-path ".cache" "bazel" "flamegraphs" $"manga-server-($stamp).svg"
}

def resolve-flamegraph-output [output: string] {
    let trimmed = ($output | str trim)

    if $trimmed == "" {
        default-flamegraph-output
    } else {
        resolve-workspace-path $trimmed
    }
}

def git-output [...args: string] {
    let result = (^git -C $SCRIPT_DIR ...$args | complete)
    if $result.exit_code == 0 {
        $result.stdout | str trim
    } else {
        ""
    }
}

def git-clean [] {
    let result = (^git -C $SCRIPT_DIR status --porcelain | complete)
    if $result.exit_code == 0 and (($result.stdout | str trim) == "") {
        "true"
    } else {
        "false"
    }
}

def server-metadata-env [] {
    let commit_sha = (git-output "rev-parse" "HEAD")
    let commit_short_sha = (git-output "rev-parse" "--short=8" "HEAD")
    let commit_at = (git-output "log" "-1" "--format=%cI")
    let branch = (git-output "rev-parse" "--abbrev-ref" "HEAD")

    {
        BUILD_TIME_3339: $commit_at
        GIT_BRANCH: $branch
        GIT_CLEAN: (git-clean)
        GIT_COMMIT_AT: $commit_at
        GIT_COMMIT_SHA: $commit_sha
        GIT_COMMIT_SHORT_SHA: $commit_short_sha
    }
}

def log-config [
    download_path: string
    plugin_dir: string
    rust_log: string
    flamegraph_output?: string
] {
    log "config" magenta ""
    log-setting "DB_PATH" $env.DB_PATH
    log-setting "SERVER_ADDR" $env.SERVER_ADDR
    log-setting "PLUGINS_PATH" $plugin_dir
    log-setting "SOURCE_PLUGIN_REGISTRY_URL" (env-status $env.SOURCE_PLUGIN_REGISTRY_URL)
    log-setting "DOWNLOAD_PATH" $download_path
    log-setting "DISCORD_BOT_TOKEN" (env-status $env.DISCORD_BOT_TOKEN --secret)
    log-setting "DISCORD_CHANNEL_ID" (env-status $env.DISCORD_CHANNEL_ID)
    log-setting "RUST_LOG" $rust_log

    if $flamegraph_output != null {
        log-setting "MANGA_FLAMEGRAPH_OUTPUT" $flamegraph_output
    }
}

def launcher-env [download_path: string, plugin_dir: string, rust_log: string] {
    server-metadata-env | merge {
        DB_PATH: $env.DB_PATH
        DOWNLOAD_PATH: $download_path
        SERVER_ADDR: $env.SERVER_ADDR
        PLUGINS_PATH: $plugin_dir
        SOURCE_PLUGIN_REGISTRY_URL: $env.SOURCE_PLUGIN_REGISTRY_URL
        DISCORD_BOT_TOKEN: $env.DISCORD_BOT_TOKEN
        DISCORD_CHANNEL_ID: $env.DISCORD_CHANNEL_ID
        RUST_LOG: $rust_log
    }
}

def aspect-start [launcher_args: list<string>] {
    exec nix develop --accept-flake-config -c aspect repo start -- ...$launcher_args
}

def aspect-flamegraph [output: string] {
    with-env { MANGA_FLAMEGRAPH_OUTPUT: $output } {
        ^nix develop --accept-flake-config -c aspect repo flamegraph -- serve
    }
}

def main [
    --cache-trace
    --flamegraph
    --flamegraph-output: string = ""
    --flamegraph-freq: int = 997
    --flamegraph-open
    --flamegraph-no-inline
] {
    ensure-nix
    prepare-default-db-path

    let download_path = (resolve-workspace-path $env.DOWNLOAD_PATH)
    let plugin_dir = (resolve-workspace-path $env.PLUGINS_PATH)
    let rust_log = (selected-rust-log $cache_trace)
    let launcher_args = if $cache_trace { ["--cache-trace"] } else { [] }

    mkdir $download_path
    mkdir $plugin_dir

    if $flamegraph {
        let flamegraph_output = (resolve-flamegraph-output $flamegraph_output)

        if $flamegraph_freq != 997 {
            log-warn "Bazel flamegraph runs use the repo's Bazel-built flamegraph target; --flamegraph-freq is no longer forwarded."
        }
        if $flamegraph_no_inline {
            log-warn "Bazel flamegraph runs use the repo's Bazel-built flamegraph target; --flamegraph-no-inline is no longer forwarded."
        }
        if $flamegraph_open {
            log-warn "Bazel flamegraph runs write MANGA_FLAMEGRAPH_OUTPUT; open the SVG after the server exits."
        }

        log-config $download_path $plugin_dir $rust_log $flamegraph_output
        log-step "Starting manga-server through the Bazel flamegraph target"

        with-env (launcher-env $download_path $plugin_dir $rust_log) {
            aspect-flamegraph $flamegraph_output
        }
        return
    }

    log-config $download_path $plugin_dir $rust_log
    log-step "Starting backend and frontend through the Bazel dev launcher"

    with-env (launcher-env $download_path $plugin_dir $rust_log) {
        aspect-start $launcher_args
    }
}
