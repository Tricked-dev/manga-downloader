def flake-output-label [name: string] {
    $".#($name)"
}

def command-error [message: string, result: record] {
    let details = [$result.stdout $result.stderr]
        | each {|value| $value | str trim }
        | where {|value| $value != "" }
        | str join "\n"

    if $details == "" {
        error make { msg: $message }
    } else {
        error make { msg: $"($message)\n($details)" }
    }
}

def nix-build-output [script_dir: string, name: string] {
    let flake_ref = $"($script_dir)#($name)"

    ^nix build $flake_ref --no-link
    if $env.LAST_EXIT_CODE != 0 {
        error make { msg: $"Error: failed to build Nix flake output .#($name)" }
    }

    let result = (^nix path-info $flake_ref | complete)
    if $result.exit_code != 0 {
        command-error $"Error: failed to resolve Nix flake output .#($name)" $result
    }

    let out_path = ($result.stdout | lines | last | str trim)
    if ($out_path | is-empty) {
        error make { msg: $"Error: Nix did not return an output path for .#($name)" }
    }

    $out_path
}

def build-plugins [script_dir: string, plugin_dir: string, plugin_output: string] {
    log-step $"Building plugins with Nix ((flake-output-label $plugin_output))"

    mkdir $plugin_dir

    let plugin_out_path = (nix-build-output $script_dir $plugin_output)
    let plugin_artifacts = (glob $"($plugin_out_path)/share/manga-server/plugins/*.wasm")
    if ($plugin_artifacts | is-empty) {
        error make { msg: $"Error: expected plugin artifact not found in ($plugin_out_path)/share/manga-server/plugins" }
    }

    for artifact_path in $plugin_artifacts {
        cp --force $artifact_path $plugin_dir
        log-ok $"Copied (($artifact_path | path basename)) to ($plugin_dir)"
    }
}

def build-aidoku-package [script_dir: string, aidoku_output: string] {
    log-step $"Building Aidoku client package with Nix ((flake-output-label $aidoku_output))"

    let package_out_path = (nix-build-output $script_dir $aidoku_output)
    let package_path = [$package_out_path "share" "manga-server" "clients" "aidoku" "package.aix"] | path join
    if not ($package_path | path exists) {
        error make { msg: $"Error: expected Aidoku package not found at ($package_path)" }
    }

    $package_path
}

def build-tachiyomi-package [script_dir: string, tachiyomi_output: string] {
    log-step $"Building Tachiyomi client package with Nix ((flake-output-label $tachiyomi_output))"

    let package_out_path = (nix-build-output $script_dir $tachiyomi_output)
    let package_path = [$package_out_path "share" "manga-server" "clients" "tachiyomi" "package.apk"] | path join
    if not ($package_path | path exists) {
        error make { msg: $"Error: expected Tachiyomi package not found at ($package_path)" }
    }

    $package_path
}

def build-runtime [
    script_dir: string
    plugin_dir: string
    server_output: string
    plugin_output: string
    aidoku_output: string
    tachiyomi_output: string
] {
    log-step $"Building manga-server with Nix ((flake-output-label $server_output))"

    let server_out_path = (nix-build-output $script_dir $server_output)
    let server_binary = [$server_out_path "bin" "manga-server"] | path join
    if not ($server_binary | path exists) {
        error make { msg: $"Error: expected server binary not found at ($server_binary)" }
    }

    build-plugins $script_dir $plugin_dir $plugin_output
    let aidoku_package_path = (build-aidoku-package $script_dir $aidoku_output)
    let tachiyomi_package_path = (build-tachiyomi-package $script_dir $tachiyomi_output)

    {
        server_binary: $server_binary
        aidoku_package_path: $aidoku_package_path
        tachiyomi_package_path: $tachiyomi_package_path
    }
}
