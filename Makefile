.PHONY: build bundle-macos signing-cert vencord-refresh

build:
	cargo +nightly build --release

bundle-macos: build
	./scripts/bundle-macos.sh

signing-cert:
	./scripts/create-signing-cert.sh

vencord-refresh:
	VENCORD_REFRESH=1 cargo +nightly build --release
