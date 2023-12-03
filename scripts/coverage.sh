#!/bin/bash
set -eox pipefail

echo ">> Building contract with coverage"

rustup target add wasm32-unknown-unknown

eval $(cargo wasmcov setup)

cargo build -p sweat_jar --target wasm32-unknown-unknown --profile=contract --features integration-test

# move the wasm file to the correct directory

make integration


# make integration

# cargo wasmcov 



