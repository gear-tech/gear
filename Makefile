.PHONY: init
init:
	@./scripts/env.sh init

.PHONY: wasm-init
wasm-init:
	@./scripts/env.sh wasm

.PHONY: js-init
js-init:
	@./scripts/env.sh js

.PHONY: show
show:
	@./scripts/env.sh show

.PHONY: docker-run
docker-run:
	@./scripts/env.sh docker

.PHONY: clippy
clippy:
	@./scripts/clippy.sh all

.PHONY: gear-clippy
gear-clippy:
	@./scripts/clippy.sh gear

.PHONY: examples-clippy
examples-clippy:
	@./scripts/clippy.sh examples

.PHONY: fmt
fmt:
	@./scripts/format.sh all

.PHONY: gear-fmt
gear-fmt:
	@./scripts/format.sh gear

.PHONY: examples-fmt
examples-fmt:
	@./scripts/format.sh examples

.PHONY: doc-fmt
doc-fmt:
	@./scripts/format.sh doc

.PHONY: check-fmt
check-fmt:
	@./scripts/format.sh all check

.PHONY: all
all:
	@./scripts/build.sh all

.PHONY: all-release
all-release:
	@./scripts/build.sh all release

.PHONY: gear
gear:
	@./scripts/build.sh gear

.PHONY: gear-release
gear-release:
	@./scripts/build.sh gear release

.PHONY: examples
examples:
	@./scripts/build.sh examples

.PHONY: node
node:
	@./scripts/build.sh node

.PHONY: node-release
node-release:
	@./scripts/build.sh node release

.PHONY: wasm-proc
wasm-proc:
	@./scripts/build.sh wasm-proc

.PHONY: check
check:
	@./scripts/build.sh all check

.PHONY: check-release
check-release:
	@./scripts/build.sh all check release

.PHONY: gear-check
gear-check:
	@./scripts/build.sh gear check

.PHONY: gear-check-release
gear-check-release:
	@./scripts/build.sh gear check release

.PHONY: examples-check
examples-check:
	@./scripts/build.sh examples check

.PHONY: test
test:
	@./scripts/test.sh all

.PHONY: test-release
test-release:
	@./scripts/test.sh all release

.PHONY: test-full
test-full:
	@./scripts/test.sh full release

.PHONY: gear-test
gear-test:
	@./scripts/test.sh gear

.PHONY: gear-test-release
gear-test-release:
	@./scripts/test.sh gear release

.PHONY: standalone-test
standalone-test:
	@./scripts/test.sh standalone

.PHONY: standalone-test-release
standalone-test-release:
	@./scripts/test.sh standalone release

.PHONY: js-test
js-test:
	@./scripts/test.sh js

.PHONY: gtest
gtest:
	@./scripts/test.sh gtest

.PHONY: gtest-v
gtest-v:
	@./scripts/test.sh gtest v

.PHONY: gtest-vv
gtest-vv:
	@./scripts/test.sh gtest vv

.PHONY: ntest
ntest:
	@./scripts/test.sh ntest

.PHONY: bench
bench:
	@./scripts/test.sh bench

.PHONY: node-run
node-run:
	@cargo run --package gear-node --release -- --dev --tmp

.PHONY: clean
clean:
	@rm -rf target
	@rm -rf examples/target
