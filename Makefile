.PHONY: build bundle-macos signing-cert

build:
	cargo +nightly build --release

bundle-macos: build
	./scripts/bundle-macos.sh

signing-cert:
	./scripts/create-signing-cert.sh
