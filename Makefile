.PHONY: build

build:
	cargo build --release
	cargo bundle
