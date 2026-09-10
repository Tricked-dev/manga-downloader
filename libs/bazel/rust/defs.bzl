"""Rust-related Bazel macro helpers."""

load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_clippy", "rustfmt_test")

def _criterion_benchmark_impl(
        name,
        visibility,
        crate_root,
        deps,
        data,
        tags):
    rust_binary(
        name = name,
        srcs = [crate_root],
        crate_root = crate_root,
        edition = "2024",
        data = data,
        deps = deps,
        tags = (tags or []) + [
            "benchmark",
            "criterion",
            "manual",
        ],
        visibility = visibility,
    )

criterion_benchmark = macro(
    attrs = {
        "crate_root": attr.string(
            mandatory = True,
            configurable = False,
            doc = "Benchmark crate root, relative to the current Bazel package.",
        ),
        "data": attr.label_list(
            default = [],
            configurable = False,
            doc = "Runtime data for the benchmark binary.",
        ),
        "deps": attr.label_list(
            default = [],
            configurable = False,
            doc = "Dependencies for the Criterion benchmark binary.",
        ),
        "tags": attr.string_list(
            default = [],
            configurable = False,
            doc = "Additional tags for the generated benchmark binary.",
        ),
    },
    implementation = _criterion_benchmark_impl,
)

def rust_quality_targets(targets, visibility = ["//visibility:public"]):
    rustfmt_test(
        name = "rustfmt_test",
        testonly = True,
        targets = targets,
        visibility = visibility,
    )

    rust_clippy(
        name = "clippy_check",
        testonly = True,
        deps = targets,
        visibility = visibility,
    )
