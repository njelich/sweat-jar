#!/bin/bash
set -eox pipefail

echo ">> Generating Code Coverage"

llvm-profdata merge -sparse integration-tests/*.profraw -o output.profdata

llvm-cov-17 show --instr-profile=output.profdata res/sweat_jar_coverage.o --format=html -output-dir=coverage/