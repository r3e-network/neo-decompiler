default: fmt test

fmt:
	cargo fmt

test:
	cargo test

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

package:
	cargo package --allow-dirty --no-verify

doc:
	cargo doc --no-deps
