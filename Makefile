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

.PHONY: clippy
clippy:
	@./scripts/clippy.sh all

.PHONY: gear-clippy
gear-clippy:
	@./scripts/clippy.sh gear

.PHONY: examples-clippy
examples-clippy:
	@./scripts/clippy.sh examples
