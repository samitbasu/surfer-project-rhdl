#!/bin/bash

cd spade/swim_tests && git submodule update --init --recursive && swim test
cd -
cp ./spade/swim_tests/build/state.ron ./examples/spade_state.ron
cp ./spade/swim_tests/build/pipeline_ready_valid_enabled_stages_behave_normally/pipeline_ready_valid.vcd ./examples/spade.vcd
