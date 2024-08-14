#!/bin/bash
set -e

SPADE_REV="53f597f25705f8b9d5dffaf8134a8e461888d2ec"

d="$(mktemp -d)"
pushd $d
git clone https://gitlab.com/spade-lang/spade
cd spade
git checkout -d $SPADE_REV
cd swim_tests
swim test pipeline_ready_valid --testcases enabled_stages_behave_normally
popd
cp $d/spade/swim_tests/build/state.ron ./examples/spade_state.ron
cp $d/spade/swim_tests/build/pipeline_ready_valid_enabled_stages_behave_normally/pipeline_ready_valid.vcd ./examples/spade.vcd

rm -rf $d
