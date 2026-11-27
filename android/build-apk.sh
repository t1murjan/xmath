#!/usr/bin/env bash
set -euo pipefail

project="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$project"

sdk="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
ndk="${ANDROID_NDK_HOME:-$sdk/ndk/27.3.13750724}"
ndk_bin="$ndk/toolchains/llvm/prebuilt/${ANDROID_NDK_HOST_TAG:-linux-x86_64}/bin"
tools="$sdk/build-tools/35.0.0"
android_jar="$sdk/platforms/android-35/android.jar"
min_sdk=24
build="$project/target/android-apk"
apk="${1:-$build/xmath-android.apk}"
keystore="${XMATH_ANDROID_KEYSTORE:-$build/xmath-local.p12}"

for tool in "$tools/aapt2" "$tools/d8" "$tools/zipalign" "$tools/apksigner" \
    "$ndk_bin/aarch64-linux-android${min_sdk}-clang" "$ndk_bin/x86_64-linux-android${min_sdk}-clang" \
    "$ndk_bin/llvm-strip"; do
    if [[ ! -x "$tool" ]]; then
        printf 'Android build tool missing: %s\n' "$tool" >&2
        exit 1
    fi
done
if [[ ! -f "$android_jar" ]]; then
    printf 'Android SDK Platform 35 missing: %s\n' "$android_jar" >&2
    exit 1
fi

mkdir -p -- "$build" "$(dirname -- "$apk")"
work="$(mktemp -d "$build/build.XXXXXXXX")"
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/classes" "$work/dex" "$work/lib/arm64-v8a" "$work/lib/x86_64"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$project/target}"
case "$CARGO_TARGET_DIR" in
    /*) ;;
    *) CARGO_TARGET_DIR="$project/$CARGO_TARGET_DIR" ;;
esac

"$tools/aapt2" compile --dir android/res -o "$work/resources.zip"
"$tools/aapt2" link --manifest android/AndroidManifest.xml -I "$android_jar" \
    -o "$work/base.apk" "$work/resources.zip"
javac -source 8 -target 8 -encoding UTF-8 -classpath "$android_jar" \
    -d "$work/classes" android/src/io/github/t1murjan/xmath/XmathActivity.java
jar_file="$work/classes.jar"
(cd "$work/classes" && zip -qr "$jar_file" .)
"$tools/d8" --min-api "$min_sdk" --lib "$android_jar" --output "$work/dex" "$jar_file"
mv "$work/dex/classes.dex" "$work/classes.dex"

build_native() {
    local target="$1"
    local abi="$2"
    local linker="$ndk_bin/$target${min_sdk}-clang"
    local underscored="${target//-/_}"
    env "CARGO_TARGET_${underscored^^}_LINKER=$linker" "CC_$underscored=$linker" \
        cargo rustc --locked --release -p xmath-gui --lib --crate-type cdylib --target "$target" \
        -- -C link-arg=-Wl,-z,max-page-size=16384
    "$ndk_bin/llvm-strip" --strip-unneeded \
        -o "$work/lib/$abi/libxmath_gui.so" "$CARGO_TARGET_DIR/$target/release/libxmath_gui.so"
}

build_native x86_64-linux-android x86_64
build_native aarch64-linux-android arm64-v8a

# Store native libraries uncompressed and align their ZIP entries for direct loading.
(cd "$work" && zip -q base.apk classes.dex && zip -q0 base.apk lib/*/*.so)
"$tools/zipalign" -P 16 -f 4 "$work/base.apk" "$work/aligned.apk"

if [[ -z "${XMATH_ANDROID_KEYSTORE:-}" ]]; then
    if [[ ! -f "$keystore" ]]; then
        keytool -genkeypair -keystore "$keystore" -storetype PKCS12 -storepass android \
            -keypass android -alias xmath-local -keyalg RSA -keysize 2048 -validity 10000 \
            -dname "CN=xmath Local Build"
    fi
    key_options=(--ks-pass pass:android --ks-key-alias xmath-local)
else
    key_options=()
fi
"$tools/apksigner" sign --ks "$keystore" "${key_options[@]}" \
    --out "$work/signed.apk" "$work/aligned.apk"
"$tools/apksigner" verify "$work/signed.apk"
"$tools/zipalign" -c -P 16 4 "$work/signed.apk"
mv -f -- "$work/signed.apk" "$apk"
printf '%s\n' "$apk"
