#!/usr/bin/env nu

const SCRIPT_DIR = (path self .)
const REPO_DIR = ($SCRIPT_DIR | path join ".." | path expand)

def paint [color: string, value: any] {
    $"(ansi $color)($value)(ansi reset)"
}

def log [label: string, color: string, message: string] {
    print $"(paint $color $"[($label)]") ($message)"
}

def log-step [message: string] {
    log "clean" cyan $message
}

def log-skip [message: string] {
    log "skip" yellow $message
}

def repo-path [...parts: string] {
    [$REPO_DIR ...$parts] | path join | path expand
}

def remove-path [path: string, dry_run: bool] {
    if not ($path | path exists) {
        return
    }

    let display_path = (try {
        $path | path relative-to $REPO_DIR
    } catch {
        $path
    })

    if $dry_run {
        log-step $"Would remove ($display_path)"
    } else {
        log-step $"Removing ($display_path)"
        rm --recursive --force $path
    }
}

def remove-glob [pattern: string, dry_run: bool] {
    glob $pattern | each {|path| remove-path $path $dry_run } | ignore
}

def clean-build [dry_run: bool] {
    remove-path (repo-path "backend" "target") $dry_run
    remove-path (repo-path "plugins" "comix" "target") $dry_run
    remove-path (repo-path ".build") $dry_run
    remove-path (repo-path "packaging" "staging") $dry_run
    remove-glob (repo-path "result*") $dry_run
}

def clean-frontend [dry_run: bool] {
    remove-path (repo-path "frontend" "node_modules") $dry_run
    remove-path (repo-path "frontend" ".svelte-kit") $dry_run
    remove-path (repo-path "frontend" "build") $dry_run
    remove-path (repo-path "frontend" "dist") $dry_run
    remove-path (repo-path "frontend" ".output") $dry_run
    remove-path (repo-path "frontend" ".vercel") $dry_run
    remove-path (repo-path "frontend" ".netlify") $dry_run
    remove-path (repo-path "frontend" ".wrangler") $dry_run
    remove-path (repo-path "frontend" ".sonda") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" "node_modules") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" ".svelte-kit") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" "build") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" ".output") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" ".vercel") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" ".netlify") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" ".wrangler") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" ".sonda") $dry_run
    remove-glob (repo-path "frontend" "vite.config.*.timestamp-*") $dry_run
    remove-glob (repo-path "frontend" "apps" "*" "vite.config.*.timestamp-*") $dry_run
}

def clean-runtime [dry_run: bool] {
    remove-path (repo-path "data") $dry_run
    remove-path (repo-path "backend" "data") $dry_run
    remove-path (repo-path "backend" "plugins") $dry_run
    remove-path (repo-path "manga.db") $dry_run
    remove-path (repo-path "manga.db-shm") $dry_run
    remove-path (repo-path "manga.db-wal") $dry_run
    remove-path (repo-path "backend" "manga.db") $dry_run
    remove-path (repo-path "backend" "manga.db-shm") $dry_run
    remove-path (repo-path "backend" "manga.db-wal") $dry_run
    remove-path (repo-path ".last_plugin_change") $dry_run
}

def clean-ccc [dry_run: bool] {
    remove-path (repo-path ".cocoindex_code") $dry_run
}

def clean-os [dry_run: bool] {
    remove-glob (repo-path "**" ".DS_Store") $dry_run
}

def main [
    --build      # Remove Rust/Nix build outputs and result symlinks.
    --frontend   # Remove frontend dependency and build outputs.
    --runtime    # Remove local runtime data, including data/ and SQLite files.
    --ccc        # Remove the local ccc index.
    --all        # Remove every cleanup category.
    --dry-run    # Print what would be removed.
] {
    let selected = [$build $frontend $runtime $ccc $all] | any {|flag| $flag }

    if not $selected {
        log-skip "Choose at least one mode: --build, --frontend, --runtime, --ccc, or --all."
        log-skip "Use --dry-run first to inspect removals."
        return
    }

    if $all or $build {
        clean-build $dry_run
    }
    if $all or $frontend {
        clean-frontend $dry_run
    }
    if $all or $runtime {
        clean-runtime $dry_run
    }
    if $all or $ccc {
        clean-ccc $dry_run
    }

    clean-os $dry_run
}
