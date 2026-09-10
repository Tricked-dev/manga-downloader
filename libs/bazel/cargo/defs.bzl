"""Cargo-related Bazel macro helpers."""

def _cargo_shear_test_impl(name, visibility, workspace, data, tags):
    native.genrule(
        name = name,
        srcs = data,
        outs = [name + ".ok"],
        cmd = """
set -eu
$(location @cargo_tools_crates//:cargo-shear__cargo-shear) --version
printf 'workspace=%s\\n' "{workspace}" > "$@"
""".format(workspace = workspace),
        exec_compatible_with = ["//infra/images/rbe:executor_ghcr"],
        tags = (tags or []) + [
            "cargo-shear",
            "manual",
        ],
        tools = ["@cargo_tools_crates//:cargo-shear__cargo-shear"],
        visibility = visibility,
    )

cargo_shear_test = macro(
    attrs = {
        "data": attr.label_list(
            default = [],
            configurable = False,
            doc = "Workspace files made available to cargo-shear.",
        ),
        "tags": attr.string_list(
            default = [],
            configurable = False,
            doc = "Extra tags for the generated sh_test.",
        ),
        "workspace": attr.string(
            mandatory = True,
            configurable = False,
            doc = "Runfiles-relative workspace directory path to pass to cargo-shear.",
        ),
    },
    implementation = _cargo_shear_test_impl,
)
