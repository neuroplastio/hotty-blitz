# Toolchain comes from mise.toml (Rust, and Node for the oracle): `mise install` first.
CARGO ?= mise x -- cargo

# The hotty repository (vectors, corpus, example programs): HOTTY_DIR, or a
# checkout next to this one.
HOTTY_DIR ?= $(abspath $(patsubst %/SPEC.md,%,$(firstword $(wildcard ../hotty/SPEC.md ../../hotty/main/SPEC.md))))
export HOTTY_DIR

.PHONY: check mods canary build release test fuzz clippy fmt corpus bench oracle blitz hotty

check: mods build test clippy test-pty fuzz   ## the gate

hotty:
	@test -n "$(HOTTY_DIR)" || { echo "no hotty checkout: set HOTTY_DIR, or clone neuroplastio/hotty next to this repository"; exit 1; }

# A set's base, from its mod.md.
base = $(shell sed -n 's/^base:[[:space:]]*\([^[:space:]\#]*\).*/\1/p' mods/$(1)/mod.md)

blitz:   ## the forks Cargo.toml patches in, from mods/ (gov P-7): Blitz at ../blitz, parley at ../parley, vello_cpu at ../vello_cpu
	@test -f ../blitz/packages/blitz-dom/src/layout/speculate.rs || scripts/blitz-fork.sh
	@test "$$(git -C ../parley rev-list --count $(call base,parley)..hotty 2>/dev/null)" = "$$(ls mods/parley/*.patch | wc -l | tr -d ' ')" || scripts/parley-fork.sh
	@test "$$(git -C ../vello_cpu rev-list --count published..hotty 2>/dev/null)" = "$$(ls mods/vello_cpu/*.patch | wc -l | tr -d ' ')" || scripts/vello-fork.sh

mods: blitz   ## mods/ as gov P-7 has it: every patch described, each base the one we build with, and each set applies on it
	@python3 scripts/mods.py check
	@test "$$(sed -n 's/.*github.com\/DioxusLabs\/blitz", rev = "\([0-9a-f]*\)".*/\1/p' Cargo.toml | sort -u)" = "$(call base,blitz)" || { echo "mods/blitz/mod.md: base is not the rev Cargo.toml pins" >&2; exit 1; }
	@test "$$(git -C ../blitz show $(call base,blitz):Cargo.toml | sed -n 's/^parley = .*rev = "\([0-9a-f]*\)".*/\1/p')" = "$(call base,parley)" || { echo "mods/parley/mod.md: base is not the rev Blitz pins" >&2; exit 1; }
	@scripts/canary.sh base

canary: blitz   ## do the sets in mods/ still apply to upstream's latest?
	@scripts/canary.sh

build: blitz
	$(CARGO) build --workspace

release: blitz
	$(CARGO) build --release --workspace

test: blitz hotty
	$(CARGO) test --workspace -q

test-pty: release hotty   ## input routing through the polyfill, this script as the terminal
	python3 tests/m3_form.py

fuzz: blitz   ## random deltas, rendered, in a release build: no panic (tests/fuzz.rs)
	$(CARGO) test --release -q -p hotty-blitz --test fuzz

clippy: blitz
	$(CARGO) clippy --workspace -q -- -D warnings

fmt:
	$(CARGO) fmt --all

corpus: release hotty   ## render every corpus page to corpus-out/*.png
	@mkdir -p corpus-out
	@for f in $(HOTTY_DIR)/corpus/*.html; do n=$$(basename $$f .html); ./target/release/hotty render $$f --cols 80 --scale 2 --cell 20x42 -o corpus-out/$$n.png --time 2>&1 | sed "s/^/$$n: /"; done

bench: release   ## delta cost vs delta size and document size
	./target/release/hotty bench --frames 30

oracle: corpus   ## corpus pages in Chromium next to hotty-blitz: corpus-out/*.sheet.png
	@./target/release/hotty css --cell 20x42 --scale 2 > corpus-out/host.css
	@args=""; for f in $(HOTTY_DIR)/corpus/*.html; do n=$$(basename $$f .html); h=$$(magick identify -format %h corpus-out/$$n.png); args="$$args $$f:$$h"; done; \
	  NODE_PATH=$$HOME/.cache/hotty/oracle/node_modules mise x node -- node scripts/oracle.cjs corpus-out/host.css corpus-out $$args
	@for f in $(HOTTY_DIR)/corpus/*.html; do n=$$(basename $$f .html); \
	  magick \( corpus-out/$$n.png -gravity north -background '#1e1e1e' -splice 0x40 -font $$(fc-match -f '%{file}' sans) -fill white -pointsize 24 -annotate +0+8 'hotty-blitz (Blitz)' \) \
	         \( corpus-out/$$n.chromium.png -gravity north -background '#1e1e1e' -splice 0x40 -font $$(fc-match -f '%{file}' sans) -fill white -pointsize 24 -annotate +0+8 'Chromium 153' \) \
	         +append corpus-out/$$n.sheet.png; done
	@echo "sheets in corpus-out/*.sheet.png"
