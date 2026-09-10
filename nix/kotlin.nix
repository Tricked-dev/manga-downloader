{ pkgs }:
# Android build-tools 35.0.1 bundles D8 8.6.2, which supports Kotlin 2.0 metadata.
# Keep the compiler and its stdlib together; newer metadata is not rewritten safely.
pkgs.kotlin.overrideAttrs (_: {
  version = "2.0.21";
  src = pkgs.fetchurl {
    url = "https://github.com/JetBrains/kotlin/releases/download/v2.0.21/kotlin-compiler-2.0.21.zip";
    hash = "sha256-A1LApFvSL4D2sm5IXNBNqAR7ql3lSGUoH7n4mkp7zyo=";
  };
})
