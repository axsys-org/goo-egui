#!/bin/sh
# Ignore developer-shell Nix compiler overrides for macOS framework linkage.
exec env CC=/usr/bin/clang CXX=/usr/bin/clang++ cargo "$@"
