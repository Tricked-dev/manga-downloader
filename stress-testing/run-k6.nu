#!/usr/bin/env nu

const SCRIPT_DIR = (path self .)
const REPO_DIR = ($SCRIPT_DIR | path join ".." | path expand)
const PROFILES = ["smoke", "bench", "stress", "bursty"]
const TESTS = [
    {
        name: "api-smoke"
        label: "API smoke"
        script: "api-smoke"
        summary: "api-smoke"
        settings_key: "api_smoke"
        profiles: ["smoke", "bench", "stress"]
        default: true
    }
    {
        name: "reader-journey"
        label: "reader journey"
        script: "reader-journey"
        summary: "reader-journey"
        settings_key: "reader_journey"
        profiles: ["smoke", "bench", "stress"]
        default: true
    }
    {
        name: "cached-pages"
        label: "cached downloaded pages"
        script: "page-cache"
        summary: "cached-pages"
        settings_key: "cached_pages"
        profiles: ["smoke", "bench", "stress"]
        default: true
    }
    {
        name: "uncached-pages"
        label: "uncached downloaded pages"
        script: "page-cache"
        summary: "uncached-pages"
        settings_key: "uncached_pages"
        profiles: ["smoke", "bench", "stress"]
        default: true
    }
    {
        name: "bursty-uncached-pages"
        label: "bursty uncached downloaded pages"
        script: "bursty-uncached-pages"
        summary: "bursty-uncached-pages"
        settings_key: "bursty_uncached_pages"
        profiles: ["bursty"]
        default: true
    }
    {
        name: "cached-media-proxy"
        label: "cached media proxy images"
        script: "media-proxy"
        summary: "cached-media-proxy"
        settings_key: "cached_media_proxy"
        profiles: ["smoke", "bench", "stress"]
        default: false
    }
    {
        name: "uncached-media-proxy"
        label: "uncached media proxy images"
        script: "media-proxy"
        summary: "uncached-media-proxy"
        settings_key: "uncached_media_proxy"
        profiles: ["smoke", "bench", "stress"]
        default: false
    }
]

def paint [color: string, value: any] {
    $"(ansi $color)($value)(ansi reset)"
}

def log [label: string, color: string, message: string] {
    print $"(paint $color $"[($label)]") ($message)"
}

def log-step [message: string] {
    log "k6" cyan $message
}

def log-ok [message: string] {
    log "ok" green $message
}

def fail [message: string] {
    error make { msg: $message }
}

def canonical-test [profile: string, raw: string] {
    let value = ($raw | str trim)
    if $value == "all" {
        return "all"
    }
    if $value == "common" {
        return "api-smoke"
    }
    if $value == "api" {
        return "api-smoke"
    }
    if $value == "reader" {
        return "reader-journey"
    }
    if $value == "cached" {
        return "cached-pages"
    }
    if $value == "uncached" {
        if $profile == "bursty" {
            return "bursty-uncached-pages"
        }
        return "uncached-pages"
    }
    if ($value == "bursty") or ($value == "uncached-pages-bursty") {
        return "bursty-uncached-pages"
    }

    $value
}

def selected-test-names [profile: string, tests: string] {
    let selected = (
        $tests
        | split row ","
        | each {|test| canonical-test $profile $test }
        | where {|test| $test != "" }
        | uniq
    )

    if ($selected | is-empty) {
        return ["all"]
    }

    let known = (($TESTS | get name) | append "all")
    let unknown = ($selected | where {|test| not ($test in $known) })
    if not ($unknown | is-empty) {
        fail $"Unknown test selection: ($unknown | str join ', '). Expected all, api-smoke, reader-journey, cached-pages, uncached-pages, bursty-uncached-pages, cached-media-proxy, uncached-media-proxy, or aliases common, api, reader, cached, uncached, bursty."
    }

    $selected
}

def selected-tests-for [profile: string, tests: string] {
    if not ($profile in $PROFILES) {
        fail $"Unknown profile '($profile)'. Expected ($PROFILES | str join ', ')."
    }

    let selected = (selected-test-names $profile $tests)
    let chosen = if "all" in $selected {
        let defaults = (
            $TESTS
            | where {|test| ($profile in $test.profiles) and $test.default }
        )
        let explicit = (
            $selected
            | where {|name| $name != "all" }
            | each {|name| $TESTS | where {|test| $test.name == $name } | first }
        )
        $defaults | append $explicit | uniq-by name
    } else {
        $selected | each {|name| $TESTS | where {|test| $test.name == $name } | first }
    }

    let unsupported = (
        $chosen
        | where {|test| not ($profile in $test.profiles) }
        | get name
    )
    if not ($unsupported | is-empty) {
        fail $"Profile '($profile)' does not support: ($unsupported | str join ', ')."
    }

    $chosen
}

def test-script [name: string] {
    [$SCRIPT_DIR $"($name).k6.ts"] | path join | path expand
}

def summary-path [summary_dir: string, name: string] {
    [$summary_dir $"k6-($name).json"] | path join | path expand
}

def dashboard-report-path [summary_dir: string, name: string] {
    [$summary_dir $"k6-($name).html"] | path join | path expand
}

def settings-for [profile: string] {
    match $profile {
        "smoke" => {
            api_smoke: { REQUESTS: 60, CONCURRENCY: 3, P95_THRESHOLD: "p(95)<500", P99_THRESHOLD: "p(99)<1000" }
            reader_journey: { REQUESTS: 20, CONCURRENCY: 2, URL_POOL: 40, PAGES_PER_JOURNEY: 2, THINK_MIN_SECONDS: 0.05, THINK_MAX_SECONDS: 0.2, P95_THRESHOLD: "p(95)<750", P99_THRESHOLD: "p(99)<1500" }
            cached_pages: { REQUESTS: 50, CONCURRENCY: 5, URL_POOL: 20, PAGE_CACHE_MODE: "cached", P95_THRESHOLD: "p(95)<100", P99_THRESHOLD: "p(99)<250" }
            uncached_pages: { REQUESTS: 40, CONCURRENCY: 5, URL_POOL: 40, PAGE_CACHE_MODE: "uncached", SKIP_PAGE_CACHE: true, P95_THRESHOLD: "p(95)<500", P99_THRESHOLD: "p(99)<1200" }
            cached_media_proxy: { REQUESTS: 30, CONCURRENCY: 3, URL_POOL: 10, MEDIA_PROXY_MODE: "cached", MEDIA_PROXY_WARMUP: true, P95_THRESHOLD: "p(95)<750", P99_THRESHOLD: "p(99)<1500" }
            uncached_media_proxy: { REQUESTS: 20, CONCURRENCY: 2, URL_POOL: 10, MEDIA_PROXY_MODE: "uncached", P95_THRESHOLD: "p(95)<2000", P99_THRESHOLD: "p(99)<3500" }
        }
        "bench" => {
            api_smoke: { REQUESTS: 1000, CONCURRENCY: 10, P95_THRESHOLD: "p(95)<250", P99_THRESHOLD: "p(99)<500" }
            reader_journey: { REQUESTS: 3000, CONCURRENCY: 24, URL_POOL: 2000, PAGES_PER_JOURNEY: 4, THINK_MIN_SECONDS: 0, THINK_MAX_SECONDS: 0.05, P95_THRESHOLD: "p(95)<250", P99_THRESHOLD: "p(99)<750" }
            cached_pages: { REQUESTS: 12000, CONCURRENCY: 32, URL_POOL: 2000, PAGE_CACHE_MODE: "cached", P95_THRESHOLD: "p(95)<100", P99_THRESHOLD: "p(99)<250" }
            uncached_pages: { REQUESTS: 8000, CONCURRENCY: 32, URL_POOL: 8000, PAGE_CACHE_MODE: "uncached", SKIP_PAGE_CACHE: true, P95_THRESHOLD: "p(95)<500", P99_THRESHOLD: "p(99)<1200" }
            cached_media_proxy: { REQUESTS: 3000, CONCURRENCY: 16, URL_POOL: 200, MEDIA_PROXY_MODE: "cached", MEDIA_PROXY_WARMUP: true, P95_THRESHOLD: "p(95)<750", P99_THRESHOLD: "p(99)<1500" }
            uncached_media_proxy: { REQUESTS: 1000, CONCURRENCY: 8, URL_POOL: 200, MEDIA_PROXY_MODE: "uncached", P95_THRESHOLD: "p(95)<2000", P99_THRESHOLD: "p(99)<3500" }
        }
        "stress" => {
            api_smoke: { DURATION: "5m", CONCURRENCY: 32, P95_THRESHOLD: "p(95)<250", P99_THRESHOLD: "p(99)<500" }
            reader_journey: { DURATION: "5m", CONCURRENCY: 64, URL_POOL: 5000, PAGES_PER_JOURNEY: 4, THINK_MIN_SECONDS: 0.05, THINK_MAX_SECONDS: 0.25, P95_THRESHOLD: "p(95)<300", P99_THRESHOLD: "p(99)<900" }
            cached_pages: { DURATION: "5m", CONCURRENCY: 64, URL_POOL: 2000, PAGE_CACHE_MODE: "cached", P95_THRESHOLD: "p(95)<100", P99_THRESHOLD: "p(99)<250" }
            uncached_pages: { DURATION: "5m", CONCURRENCY: 64, URL_POOL: 20000, PAGE_CACHE_MODE: "uncached", SKIP_PAGE_CACHE: true, P95_THRESHOLD: "p(95)<600", P99_THRESHOLD: "p(99)<1500" }
            cached_media_proxy: { DURATION: "5m", CONCURRENCY: 32, URL_POOL: 1000, MEDIA_PROXY_MODE: "cached", MEDIA_PROXY_WARMUP: true, P95_THRESHOLD: "p(95)<750", P99_THRESHOLD: "p(99)<1500" }
            uncached_media_proxy: { DURATION: "5m", CONCURRENCY: 16, URL_POOL: 1000, MEDIA_PROXY_MODE: "uncached", P95_THRESHOLD: "p(95)<2200", P99_THRESHOLD: "p(99)<4000" }
        }
        "bursty" => {
            bursty_uncached_pages: { CONCURRENCY: 128, URL_POOL: 30000, SKIP_PAGE_CACHE: true, BASELINE_RPS: 3000, BURST_RPS: 10000, TROUGH_RPS: 15, BURST_CYCLES: 12, BURST_RAMP_DURATION: "5s", BURST_HOLD_DURATION: "10s", BURST_REST_DURATION: "5s", BURST_COOLDOWN_DURATION: "0s", PRE_ALLOCATED_VUS: 128, MAX_VUS: 600, P95_THRESHOLD: "p(95)<600", P99_THRESHOLD: "p(99)<1500" }
        }
    }
}

def k6-env [
    server: string
    api_key: string
    summary_dir: string
    name: string
    settings: record
] {
    mut env_vars = {
        SERVER: $server
        API_KEY: $api_key
        BACKEND_API_KEY: $api_key
        SUMMARY_PATH: (summary-path $summary_dir $name)
    }

    for key in ($settings | columns) {
        let value = ($settings | get $key)
        if ($value | into string) != "" {
            $env_vars = ($env_vars | upsert $key ($value | into string))
        }
    }

    $env_vars
}

def dashboard-env [
    env_vars: record
    summary_dir: string
    name: string
    web_dashboard: bool
    web_dashboard_open: bool
    web_dashboard_host: string
    web_dashboard_port: int
    web_dashboard_period: string
    web_dashboard_export: bool
] {
    if not $web_dashboard {
        return $env_vars
    }

    mut dashboard_vars = {
        K6_WEB_DASHBOARD: "true"
        K6_WEB_DASHBOARD_OPEN: ($web_dashboard_open | into string)
        K6_WEB_DASHBOARD_HOST: $web_dashboard_host
        K6_WEB_DASHBOARD_PORT: ($web_dashboard_port | into string)
        K6_WEB_DASHBOARD_PERIOD: $web_dashboard_period
    }

    if $web_dashboard_export {
        $dashboard_vars = ($dashboard_vars | merge {
            K6_WEB_DASHBOARD_EXPORT: (dashboard-report-path $summary_dir $name)
        })
    }

    $env_vars | merge $dashboard_vars
}

def assert-k6 [] {
    let result = (^k6 version | complete)
    if $result.exit_code != 0 {
        fail "k6 is not available on PATH. Install k6 globally, then rerun this script."
    }

    log-ok ($result.stdout | str trim)
}

def check-backend [server: string] {
    let result = (^curl -fsS --max-time 5 $"($server)/v1/health" | complete)
    if $result.exit_code != 0 {
        fail $"Backend health check failed at ($server)/v1/health. Start the backend separately before running k6."
    }

    log-ok $"Backend is reachable at ($server)"
}

def run-test [
    test: record
    settings: record
    server: string
    api_key: string
    summary_dir: string
    web_dashboard: bool
    web_dashboard_open: bool
    web_dashboard_host: string
    web_dashboard_port: int
    web_dashboard_period: string
    web_dashboard_export: bool
] {
    let script = (test-script $test.script)
    let base_env = (k6-env $server $api_key $summary_dir $test.summary $settings)
    let env_vars = (dashboard-env $base_env $summary_dir $test.summary $web_dashboard $web_dashboard_open $web_dashboard_host $web_dashboard_port $web_dashboard_period $web_dashboard_export)

    log-step $"Running ($test.label): ($script)"
    log-step $"Summary: ($env_vars.SUMMARY_PATH)"
    if (($env_vars | columns) | any {|key| $key == "K6_WEB_DASHBOARD" }) {
        log-step $"Web dashboard: http://($env_vars.K6_WEB_DASHBOARD_HOST):($env_vars.K6_WEB_DASHBOARD_PORT)"
        if (($env_vars | columns) | any {|key| $key == "K6_WEB_DASHBOARD_EXPORT" }) {
            log-step $"HTML report: ($env_vars.K6_WEB_DASHBOARD_EXPORT)"
        }
    }

    with-env $env_vars {
        ^k6 run $script
    }

    if $env.LAST_EXIT_CODE != 0 {
        fail $"k6 test failed: ($test.name)"
    }

    log-ok $"Finished ($test.label)"
}

def run-suite [
    profile: string
    tests: string
    server: string
    api_key: string
    summary_dir: string
    skip_health_check: bool
    web_dashboard: bool
    web_dashboard_open: bool
    web_dashboard_host: string
    web_dashboard_port: int
    web_dashboard_period: string
    web_dashboard_export: bool
] {
    cd $REPO_DIR

    let selected_tests = (selected-tests-for $profile $tests)
    let profile_settings = (settings-for $profile)
    mkdir $summary_dir

    log-step $"Profile: ($profile)"
    log-step $"Tests: (($selected_tests | get name) | str join ', ')"

    assert-k6
    if not $skip_health_check {
        check-backend $server
    }

    for test in $selected_tests {
        let settings = ($profile_settings | get $test.settings_key)
        run-test $test $settings $server $api_key $summary_dir $web_dashboard $web_dashboard_open $web_dashboard_host $web_dashboard_port $web_dashboard_period $web_dashboard_export
    }
}

def main [
    --profile: string = "bench"        # smoke, bench, stress, or bursty.
    --tests: string = "all"            # all, api-smoke, reader-journey, cached-pages, uncached-pages, bursty-uncached-pages, cached-media-proxy, uncached-media-proxy, or aliases common, api, reader, cached, uncached, bursty.
    --server: string = "http://127.0.0.1:4000"
    --api-key: string = "123"
    --summary-dir: string = "/tmp/manga-bench"
    --skip-health-check                # Do not call /v1/health before starting.
    --web-dashboard                    # Enable k6 dashboard.
    --web-dashboard-open               # Ask k6 to open the dashboard in the default browser.
    --web-dashboard-host: string = "127.0.0.1"
    --web-dashboard-port: int = 5665
    --web-dashboard-period: string = "1s"
    --web-dashboard-export             # Export one HTML report per test to the summary directory.
] {
    run-suite $profile $tests $server $api_key $summary_dir $skip_health_check $web_dashboard $web_dashboard_open $web_dashboard_host $web_dashboard_port $web_dashboard_period $web_dashboard_export
}

def "main dashboard" [
    --profile: string = "bench"        # smoke, bench, stress, or bursty.
    --tests: string = "all"            # all, api-smoke, reader-journey, cached-pages, uncached-pages, bursty-uncached-pages, cached-media-proxy, uncached-media-proxy, or aliases common, api, reader, cached, uncached, bursty.
    --server: string = "http://127.0.0.1:4000"
    --api-key: string = "123"
    --summary-dir: string = "/tmp/manga-bench"
    --skip-health-check                # Do not call /v1/health before k6.
    --open                             # Ask k6 to open the dashboard in the default browser.
    --host: string = "127.0.0.1"
    --port: int = 5665
    --period: string = "1s"
    --export                           # Export one HTML report per test to the summary directory.
] {
    run-suite $profile $tests $server $api_key $summary_dir $skip_health_check true $open $host $port $period $export
}
