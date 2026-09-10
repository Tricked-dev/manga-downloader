# Build Artifacts

`build-bazel-sysupdate.sh` creates a temporary copy of this flake and writes the
Bazel-built `manga-server` executable here before building the sysupdate bundle.

Do not commit generated binaries from this directory.
