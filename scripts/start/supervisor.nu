def run-supervised [
    script_path: string
    plugin_dir: string
    server_binary: string
    aidoku_package_path: string
    tachiyomi_package_path: string
    rust_log: string
    metadata_env: record
    flamegraph_config: record
] {
    let supervisor_script = "
set -u

plugin_dir=\$1
server_binary=\$2
flamegraph_enabled=\$3
flamegraph_output=\$4
flamegraph_freq=\$5
flamegraph_open=\$6
flamegraph_no_inline=\$7

watch_pid=''
server_pid=''
xctrace_wrapper_dir=''
flamegraph_work_dir=''

cleanup() {
  local status=\$?

  trap - EXIT INT TERM

  [[ -n \"\$server_pid\" ]] && kill \"\$server_pid\" 2>/dev/null || true
  [[ -n \"\$watch_pid\" ]] && kill \"\$watch_pid\" 2>/dev/null || true
  [[ -n \"\$xctrace_wrapper_dir\" ]] && rm -rf \"\$xctrace_wrapper_dir\" 2>/dev/null || true
  [[ -n \"\$flamegraph_work_dir\" ]] && rm -rf \"\$flamegraph_work_dir\" 2>/dev/null || true
  wait 2>/dev/null || true

  exit \"\$status\"
}

trap cleanup EXIT INT TERM

nu \"$0\" watch-plugins \"\$plugin_dir\" &
watch_pid=\$!

if [[ \"\$flamegraph_enabled\" == \"true\" ]]; then
  repo_dir=\$(cd \"\$(dirname \"$0\")\" && pwd)
  flamegraph_work_dir=\$(mktemp -d \"\${TMPDIR:-/tmp}/manga-flamegraph-run.XXXXXX\")
  mkdir -p \"\$(dirname \"\$flamegraph_output\")\"

  flamegraph_args=(--output \"\$flamegraph_output\" --freq \"\$flamegraph_freq\")
  [[ \"\$flamegraph_open\" == \"true\" ]] && flamegraph_args+=(--open)
  [[ \"\$flamegraph_no_inline\" == \"true\" ]] && flamegraph_args+=(--no-inline)

  if [[ \"\$(uname -s)\" == \"Darwin\" ]]; then
    command -v python3 >/dev/null || {
      echo \"Error: macOS flamegraph XML workaround requires python3 on PATH.\" >&2
      exit 1
    }

    xctrace_wrapper_dir=\$(mktemp -d \"\${TMPDIR:-/tmp}/manga-flamegraph-xctrace.XXXXXX\")
    xctrace_normalizer=\"\$xctrace_wrapper_dir/normalize_xctrace.py\"
    cat >\"\$xctrace_normalizer\" <<'EOF'
#!/usr/bin/env python3
import html
import sys
import xml.etree.ElementTree as ET


root = ET.fromstring(sys.stdin.buffer.read())

frame_names = {}
frame_ids = []
backtrace_ids = []

for frame in root.iter(\"frame\"):
    frame_id = frame.attrib.get(\"id\")
    frame_name = frame.attrib.get(\"name\")
    if frame_id is not None:
        frame_ids.append(int(frame_id))
    if frame_id is not None and frame_name is not None:
        frame_names[frame_id] = frame_name


def names_for_backtrace(backtrace):
    names = []
    for frame in list(backtrace):
        if frame.tag != \"frame\":
            continue

        name = frame.attrib.get(\"name\")
        if name is None:
            ref = frame.attrib.get(\"ref\")
            name = frame_names.get(ref, f\"<frame ref={ref}>\")
        names.append(name)
    return names


backtrace_names = {}
for backtrace in root.iter(\"backtrace\"):
    backtrace_id = backtrace.attrib.get(\"id\")
    if backtrace_id is not None:
        backtrace_ids.append(int(backtrace_id))
        backtrace_names[backtrace_id] = names_for_backtrace(backtrace)

next_frame_id = max(frame_ids, default=0) + 1
next_backtrace_id = max(backtrace_ids, default=0) + 1


def fresh_backtrace(names):
    global next_frame_id
    global next_backtrace_id

    backtrace = ET.Element(\"backtrace\", {\"id\": str(next_backtrace_id)})
    next_backtrace_id += 1

    for name in names:
        frame = ET.SubElement(
            backtrace,
            \"frame\",
            {\"id\": str(next_frame_id), \"name\": name},
        )
        next_frame_id += 1

    return backtrace


for row in root.iter(\"row\"):
    for index, child in enumerate(list(row)):
        if child.tag != \"backtrace\":
            continue

        ref = child.attrib.get(\"ref\")
        names = backtrace_names.get(ref, []) if ref is not None else names_for_backtrace(child)
        row.remove(child)
        row.insert(index, fresh_backtrace(names))
        break


def write_element(element, output):
    tag = element.tag
    attrs = \"\".join(
        f' {key}=\"{html.escape(value, quote=True)}\"'
        for key, value in element.attrib.items()
    )

    output.write(f\"<{tag}{attrs}>\")
    if element.text:
        output.write(html.escape(element.text, quote=False))

    for child in list(element):
        write_element(child, output)
        if child.tail:
            output.write(html.escape(child.tail, quote=False))

    output.write(f\"</{tag}>\")


sys.stdout.write('<?xml version=\"1.0\"?>\\n')
write_element(root, sys.stdout)
sys.stdout.write('\\n')
EOF

    xctrace_wrapper=\"\$xctrace_wrapper_dir/xctrace\"
    cat >\"\$xctrace_wrapper\" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

real_xctrace=\"\${REAL_XCTRACE:-/usr/bin/xctrace}\"

if [[ \"\${1:-}\" == \"export\" ]]; then
  \"\$real_xctrace\" \"\$@\" | python3 \"\$XCTRACE_NORMALIZER\"
else
  exec \"\$real_xctrace\" \"\$@\"
fi
EOF
    chmod +x \"\$xctrace_wrapper\"

    real_xctrace=\"\${XCTRACE:-}\"
    if [[ -z \"\$real_xctrace\" || \"\$real_xctrace\" == \"\$xctrace_wrapper\" ]]; then
      real_xctrace=\$(command -v xctrace)
    fi

    export REAL_XCTRACE=\"\$real_xctrace\"
    export XCTRACE_NORMALIZER=\"\$xctrace_normalizer\"
    export XCTRACE=\"\$xctrace_wrapper\"
  fi

  (
    cd \"\$flamegraph_work_dir\"
    flamegraph \"\${flamegraph_args[@]}\" -- /bin/bash -c 'cd \"\$1\" && exec \"\$2\" serve' bash \"\$repo_dir\" \"\$server_binary\"
  )
else
  \"\$server_binary\" serve &
  server_pid=\$!

  wait \"\$server_pid\"
fi
"

    if $flamegraph_config.enabled {
        log-step $"Starting manga-server through flamegraph; SVG will be written when the server exits: ($flamegraph_config.output)"
    } else {
        log-step "Starting manga-server"
    }
    with-env ($metadata_env | merge {
        AIDOKU_PACKAGE_PATH: $aidoku_package_path
        TACHIYOMI_PACKAGE_PATH: $tachiyomi_package_path
        RUST_LOG: $rust_log
    }) {
        exec bash -c $supervisor_script $script_path $plugin_dir $server_binary $flamegraph_config.enabled $flamegraph_config.output $flamegraph_config.frequency $flamegraph_config.open $flamegraph_config.no_inline
    }
}
