#!/usr/bin/env bash
# Builds a development toolchain folder for compiled programs (specification 44 §4.6): the pinned
# rustc and rust-lld of the current rustup toolchain, and the interface and program library crates
# built for each target with the module flags of §4.5. The build finds the folder at
# TRUST_TOOLCHAIN. `trust-runtime toolchain bundle <folder> <archive>` turns the folder into the
# toolchain bundle a release ships (§4.6).
#
# The avr-none module target (specification 34 D.6, specification 44 §4.4) is tier 3, which no
# stable release ships a standard library for, so it builds with the pinned nightly
# (AVR_TOOLCHAIN, `nightly` by default): its `core` and `compiler_builtins` go into the folder's
# sysroot layout `lib/rustlib/avr-none/lib/`, the libraries into `lib/avr-none/`, and the nightly
# itself is linked as `bin/rustc-nightly` with its linker as `bin/rust-lld-nightly`.
#
# Usage: scripts/native/build_toolchain_libraries.sh <folder> [<triple>...]
#   The triples default to every module target of specification 44 §4.4, as a bundle holds them.
set -euo pipefail
out=${1:?usage: build_toolchain_libraries.sh <folder> [<triple>...]}
shift
if [ $# -gt 0 ]; then
  triples=("$@")
else
  triples=(riscv32imac-unknown-none-elf thumbv8m.main-none-eabihf x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu avr-none)
fi
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
sysroot=$(rustc --print sysroot)
host=$(rustc -vV | sed -n 's/^host: //p')
avr_toolchain=${AVR_TOOLCHAIN:-nightly}
mkdir -p "$out/lib"
# The compiler, linked where the build looks for it. Not on Windows: rustc.exe runs only beside its
# own libraries, so a Windows computer takes the bundle `trust-runtime toolchain bundle` writes from
# this folder and `trust-runtime toolchain install --from` installs (§4.6).
case $host in
  *-windows-*) ;;
  *-apple-darwin)
    # macOS's rust-lld loads LLVM from ../lib beside its own file, so it is copied, not linked,
    # next to the compiler's libraries, as the bundle places them; a cleared environment, as an
    # isolated run has (protocol annex E8.1), then still finds them.
    mkdir -p "$out/bin"
    ln -sfn "$sysroot/bin/rustc" "$out/bin/rustc"
    rm -f "$out/bin/rust-lld"
    cp "$sysroot/lib/rustlib/$host/bin/rust-lld" "$out/bin/rust-lld"
    for library in "$sysroot"/lib/*.dylib; do ln -sfn "$library" "$out/lib/$(basename "$library")"; done
    ;;
  *)
    mkdir -p "$out/bin"
    ln -sfn "$sysroot/bin/rustc" "$out/bin/rustc"
    ln -sfn "$sysroot/lib/rustlib/$host/bin/rust-lld" "$out/bin/rust-lld"
    # The pinned nightly of the avr-none module target, with its own linker (§4.5, §4.6).
    avr_sysroot=$(rustc +"$avr_toolchain" --print sysroot)
    ln -sfn "$avr_sysroot/bin/rustc" "$out/bin/rustc-nightly"
    ln -sfn "$avr_sysroot/lib/rustlib/$host/bin/rust-lld" "$out/bin/rust-lld-nightly"
    ;;
esac
for triple in "${triples[@]}"; do
  build="$out/build"
  # The module flags of specification 44 §4.5 that the libraries share with the module:
  # position-independent code, abort on panic, bitcode for the module's link-time optimization,
  # and no RELRO, so that the memory functions are called directly, not through a global offset
  # table (the flag travels in the bitcode).
  # A computer's module links no C library, so it takes the memory functions from
  # trust-plc-mem; a chip's target builtins have them.
  packages=(-p trust-plc-lib)
  crates=(trust_plc_abi trust_plc_lib libm ryu)
  case $triple in
    *-none*) ;;
    *) packages+=(-p trust-plc-mem); crates+=(trust_plc_mem) ;;
  esac
  flags="-C relocation-model=pic -C panic=abort -C embed-bitcode=yes -C opt-level=2 -C codegen-units=1 -C relro-level=off"
  # The AArch64 module reserves x18, which macOS reserves and may overwrite when it switches
  # threads (§4.4). Link-time optimization keeps each function's own target features, so the
  # libraries reserve it too: a library function that kept a value in x18 computed a wrong value
  # or crashed at random on macOS.
  case $triple in
    aarch64-*) flags="$flags -C target-feature=+reserve-x18" ;;
  esac
  # The newest rlib cargo built for `crate`: normally in deps/, but with -Z build-std (avr-none)
  # cargo puts every crate under build/<crate>/<hash>/out/ instead.
  newest_rlib() {
    local dashed=${1//_/-}
    ls -t "$build/$triple/release/deps/lib$1-"*.rlib \
          "$build/$triple/release/build/$1/"*/out/lib"$1"-*.rlib \
          "$build/$triple/release/build/$dashed/"*/out/lib"$1"-*.rlib 2>/dev/null | head -1 || true
  }
  if [ "$triple" = "avr-none" ]; then
    # avr-none is tier 3, which rustup ships no standard library for, and it requires an explicit
    # target-cpu, the avr5 baseline every AVR of the family runs (specification 34 D.6). The
    # per-target variable keeps the flags off the host's build scripts. Two steps: the nightly
    # builds core and compiler_builtins (with the memory functions, which the AVR has no
    # instructions for) as the rlib+rmeta pair a sysroot's rustlib folder holds, then the
    # libraries build against that sysroot as normal full rlibs in deps/.
    avr_flags="$flags -C target-cpu=atmega328p"
    CARGO_TARGET_AVR_NONE_RUSTFLAGS="$avr_flags" \
      cargo +"$avr_toolchain" build --quiet --release -p trust-plc-abi --target "$triple" \
        --target-dir "$build" \
        -Z build-std=core,compiler_builtins -Z build-std-features=compiler-builtins-mem
    mkdir -p "$out/lib/rustlib/avr-none/lib"
    for crate in core compiler_builtins; do
      newest=$(newest_rlib "$crate")
      if [ -z "$newest" ]; then
        echo "no lib$crate rlib built for $triple under $build" >&2
        exit 1
      fi
      cp "$newest" "$out/lib/rustlib/avr-none/lib/"
      rmeta="${newest%.rlib}.rmeta"
      if [ -f "$rmeta" ]; then
        cp "$rmeta" "$out/lib/rustlib/avr-none/lib/"
      fi
    done
    CARGO_TARGET_AVR_NONE_RUSTFLAGS="$avr_flags --sysroot $out" \
      cargo +"$avr_toolchain" build --quiet --release "${packages[@]}" --target "$triple" \
        --target-dir "$build"
  else
    RUSTFLAGS="$flags" \
      cargo build --quiet --release "${packages[@]}" --target "$triple" --target-dir "$build"
  fi
  libraries="$out/lib/$triple"
  mkdir -p "$libraries"
  rm -f "$libraries"/*.rlib
  # The newest build of each crate the library links, so that an older build left in the
  # target folder is no second candidate.
  for crate in "${crates[@]}"; do
    newest=$(newest_rlib "$crate")
    if [ -z "$newest" ]; then
      echo "no lib$crate rlib built for $triple under $build" >&2
      exit 1
    fi
    cp "$newest" "$libraries/"
  done
  # The program library the module links, which the image's header names by a hash and the
  # kernel of the same release admits (specification 44 §5.2, §10.2).
  version=$(cargo pkgid -p trust-plc-lib | sed 's/.*[#@]//')
  echo "trust-plc-lib $version" > "$libraries/LIBRARY"
  echo "$triple: $(ls "$libraries" | tr '\n' ' ')"
done
