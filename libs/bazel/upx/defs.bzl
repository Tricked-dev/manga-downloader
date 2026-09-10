"""UPX compression rules."""

def _upx_compress_binary_resource_set(os, inputs_size):
    return {
        "cpu": 6,
        "memory": 4096,
    }

def _upx_compress_binary_impl(ctx):
    output = ctx.actions.declare_file(ctx.attr.output_name or ctx.label.name)

    args = ctx.actions.args()
    args.add(ctx.attr.compression)
    args.add_all(ctx.attr.upx_args)
    args.add("-o")
    args.add(output)
    args.add(ctx.executable.binary)

    ctx.actions.run(
        executable = ctx.executable.upx,
        inputs = [ctx.executable.binary],
        outputs = [output],
        arguments = [args],
        mnemonic = "UpxCompressBinary",
        progress_message = "UPX-compressing %{input}",
        resource_set = _upx_compress_binary_resource_set,
        tools = [ctx.executable.upx],
    )

    binary_info = ctx.attr.binary[DefaultInfo]
    runfiles = ctx.runfiles().merge(binary_info.default_runfiles).merge(binary_info.data_runfiles)
    return [DefaultInfo(
        executable = output,
        files = depset([output]),
        runfiles = runfiles,
    )]

upx_compress_binary = rule(
    implementation = _upx_compress_binary_impl,
    attrs = {
        "binary": attr.label(
            cfg = "target",
            doc = "Executable target to compress with UPX.",
            executable = True,
            mandatory = True,
        ),
        "output_name": attr.string(
            doc = "Optional output filename. Defaults to the target name.",
        ),
        "compression": attr.string(
            default = "-9",
            doc = "UPX compression mode. Use --best, --brute, or --ultra-brute for slower exhaustive modes.",
            values = [
                "-1",
                "-2",
                "-3",
                "-4",
                "-5",
                "-6",
                "-7",
                "-8",
                "-9",
                "--best",
                "--brute",
                "--ultra-brute",
            ],
        ),
        "upx": attr.label(
            cfg = "exec",
            default = Label("@upx_src//:upx"),
            doc = "UPX executable used for compression.",
            executable = True,
        ),
        "upx_args": attr.string_list(
            doc = "Additional arguments passed to UPX after the compression mode and before -o <output> <input>.",
        ),
    },
    doc = "Compresses an executable with source-built UPX.",
    executable = True,
)
