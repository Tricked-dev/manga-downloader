#!/usr/bin/env bash
set -euo pipefail

out="${1:-build/package.apk}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
build_dir="$root/build"
stubs_classes="$build_dir/stubs-classes"
source_classes="$build_dir/source-classes"
compiled_res="$build_dir/compiled-res"
generated="$build_dir/generated"

android_home="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [[ -z "$android_home" ]]; then
	echo "error: ANDROID_HOME or ANDROID_SDK_ROOT must point to an Android SDK" >&2
	exit 1
fi

android_jar="$(find -L "$android_home/platforms" -path '*/android.jar' | sort -V | tail -n 1)"
build_tools="$(find -L "$android_home/build-tools" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n 1)"
if [[ -z "$android_jar" || -z "$build_tools" ]]; then
	echo "error: Android SDK is missing platforms or build-tools under $android_home" >&2
	exit 1
fi

aapt2="$build_tools/aapt2"
d8="$build_tools/d8"
zipalign="$build_tools/zipalign"
apksigner="$build_tools/apksigner"

rm -rf "$build_dir"
mkdir -p "$stubs_classes" "$source_classes" "$compiled_res" "$generated" "$(dirname "$out")"

mapfile -t stub_sources < <(find "$root/stubs" -name '*.java' | sort)
javac -source 8 -target 8 \
	-bootclasspath "$android_jar" \
	-d "$stubs_classes" \
	"${stub_sources[@]}"

mapfile -t kotlin_sources < <(find "$root/src" -name '*.kt' | sort)
mapfile -t java_sources < <(find "$root/src" -name '*.java' | sort)
kotlin_runtime_jars=()

if ((${#kotlin_sources[@]} > 0)); then
	kotlinc_bin="$(command -v kotlinc || true)"
	if [[ -z "$kotlinc_bin" ]]; then
		echo "error: kotlinc is required to build Kotlin Tachiyomi sources" >&2
		exit 1
	fi

	"$kotlinc_bin" \
		-jvm-target 1.8 \
		-classpath "$android_jar:$stubs_classes" \
		-d "$source_classes" \
		"${kotlin_sources[@]}"

	kotlin_root="$(cd "$(dirname "$kotlinc_bin")/.." && pwd)"
	for jar_name in kotlin-stdlib.jar kotlin-stdlib-jdk7.jar kotlin-stdlib-jdk8.jar; do
		jar_path="$(find -L "$kotlin_root" -type f -name "$jar_name" | head -n 1)"
		if [[ -n "$jar_path" ]]; then
			kotlin_runtime_jars+=("$jar_path")
		fi
	done
	if ((${#kotlin_runtime_jars[@]} == 0)); then
		echo "error: Kotlin stdlib jars were not found under $kotlin_root" >&2
		exit 1
	fi
fi

if ((${#java_sources[@]} > 0)); then
	javac -source 8 -target 8 \
		-bootclasspath "$android_jar" \
		-classpath "$stubs_classes:$source_classes" \
		-d "$source_classes" \
		"${java_sources[@]}"
fi

"$aapt2" compile --dir "$root/res" -o "$compiled_res/resources.zip"
"$aapt2" link \
	-I "$android_jar" \
	--manifest "$root/AndroidManifest.xml" \
	--java "$generated" \
	-o "$build_dir/unsigned.apk" \
	"$compiled_res/resources.zip"

mkdir -p "$build_dir/dex"
mapfile -t class_files < <(find "$source_classes" -name '*.class' | sort)
"$d8" \
	--min-api 21 \
	--lib "$android_jar" \
	--classpath "$stubs_classes" \
	--output "$build_dir/dex" \
	"${class_files[@]}" \
	"${kotlin_runtime_jars[@]}"

(cd "$build_dir/dex" && zip -q "$build_dir/unsigned.apk" classes.dex)

key_store="$build_dir/debug.keystore"
if ! base64 --decode "$root/signing/debug.keystore.b64" >"$key_store" 2>/dev/null; then
	base64 -D -i "$root/signing/debug.keystore.b64" -o "$key_store"
fi

"$zipalign" -f 4 "$build_dir/unsigned.apk" "$build_dir/aligned.apk"
"$apksigner" sign \
	--ks "$key_store" \
	--ks-pass pass:android \
	--key-pass pass:android \
	--out "$out" \
	"$build_dir/aligned.apk"

"$apksigner" verify "$out"
