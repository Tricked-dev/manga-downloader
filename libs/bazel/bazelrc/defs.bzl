"""Hermetic Bazel policy checks."""

def _encode_checks(checks):
    return ["%s\t%s" % (name, snippet) for name, snippet in checks]

def _bazel_policy_check_impl(ctx):
    output = ctx.actions.declare_file("%s.ok" % ctx.label.name)
    args = ctx.actions.args()
    args.add("--output", output)
    args.add("--policy", ctx.attr.policy)

    inputs = []
    for src in ctx.files.srcs:
        args.add("--input", src)
        inputs.append(src)
    for src in ctx.files.version_inputs:
        args.add("--version-input", src)
        inputs.append(src)
    for src in ctx.files.external_label_inputs:
        args.add("--external-label-input", src)
        inputs.append(src)
    for src in ctx.files.yaml_entrypoint_inputs:
        args.add("--yaml-entrypoint-input", src)
        inputs.append(src)
    for src in ctx.files.duplicate_inputs:
        args.add("--duplicate-input", src)
        inputs.append(src)

    for check in ctx.attr.requires:
        args.add("--require", check)
    for check in ctx.attr.forbids:
        args.add("--forbid", check)

    if ctx.attr.no_adjacent_duplicate_nonempty:
        args.add("--no-adjacent-duplicate-nonempty")
    if ctx.attr.allow_only_basename:
        args.add("--allow-only-basename", ctx.attr.allow_only_basename)

    ctx.actions.run(
        mnemonic = "BazelPolicyCheck",
        executable = ctx.executable._runner,
        inputs = depset(inputs),
        outputs = [output],
        arguments = [args],
        progress_message = "Checking Bazel policy %{label}",
    )

    return [DefaultInfo(files = depset([output]))]

_bazel_policy_check = rule(
    implementation = _bazel_policy_check_impl,
    attrs = {
        "allow_only_basename": attr.string(
            doc = "If set, every declared --input must have this basename.",
        ),
        "external_label_inputs": attr.label_list(
            allow_files = True,
            doc = "Files scanned for version-stamped external labels.",
        ),
        "duplicate_inputs": attr.label_list(
            allow_files = True,
            doc = "Files scanned for adjacent duplicate non-empty lines.",
        ),
        "forbids": attr.string_list(
            doc = "Tab-delimited check name and forbidden snippet pairs.",
        ),
        "no_adjacent_duplicate_nonempty": attr.bool(
            doc = "Reject adjacent duplicate non-empty lines across inputs.",
        ),
        "policy": attr.string(
            mandatory = True,
            doc = "Human-readable policy name used in diagnostics.",
        ),
        "requires": attr.string_list(
            doc = "Tab-delimited check name and required snippet pairs.",
        ),
        "srcs": attr.label_list(
            allow_files = True,
            doc = "Files searched for required and forbidden snippets.",
        ),
        "version_inputs": attr.label_list(
            allow_files = True,
            doc = "Files scanned for BUILD-level version pins.",
        ),
        "yaml_entrypoint_inputs": attr.label_list(
            allow_files = True,
            doc = "YAML files scanned for entrypoint/cmd keys.",
        ),
        "_runner": attr.label(
            default = Label("//libs/bazel/bazelrc:policy_runner"),
            executable = True,
            cfg = "exec",
        ),
    },
)

def bazel_policy_check(
        name,
        policy,
        srcs = [],
        requires = [],
        forbids = [],
        duplicate_inputs = [],
        version_inputs = [],
        external_label_inputs = [],
        yaml_entrypoint_inputs = [],
        no_adjacent_duplicate_nonempty = False,
        allow_only_basename = "",
        exec_compatible_with = [],
        tags = [],
        visibility = None):
    _bazel_policy_check(
        name = name,
        allow_only_basename = allow_only_basename,
        exec_compatible_with = exec_compatible_with,
        duplicate_inputs = duplicate_inputs,
        external_label_inputs = external_label_inputs,
        forbids = _encode_checks(forbids),
        no_adjacent_duplicate_nonempty = no_adjacent_duplicate_nonempty,
        policy = policy,
        requires = _encode_checks(requires),
        srcs = srcs,
        tags = tags,
        version_inputs = version_inputs,
        visibility = visibility,
        yaml_entrypoint_inputs = yaml_entrypoint_inputs,
    )
