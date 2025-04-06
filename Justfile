book-src := justfile_directory() / 'docs' / 'book'

CI-SET-ENV := 'export RUSTFLAGS="-Dwarnings" RUSTDOCFLAGS="-Dwarnings"'

[private]
default:
	@just --unsorted --list

# compile and run tytanic
run *args='--release':
	cargo run {{ args }}

# run lints project wide
check:
	# FIXME(tinger): mdbook-linkcheck is disabled, because some links are
	# deliberately pointing to the generated html files, see:
	# https://github.com/rust-lang/mdBook/issues/984
	# mdbook-linkcheck --standalone {{ book-src }}
	mdbook test {{ book-src }}
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features

# run tests project wide
test:
	cargo insta test --test-runner nextest --workspace

# build and serve the book locally
book *args='--open':
	mdbook serve {{ book-src }} {{ args }}

# run tests and checks similar to CI
ci:
	{{ CI-SET-ENV }} && cargo fmt --all --check
	{{ CI-SET-ENV }} && cargo clippy --workspace
	{{ CI-SET-ENV }} && cargo test --workspace
	{{ CI-SET-ENV }} && cargo doc --workspace --no-deps
	@echo ""
	@echo These checks are not exactly the same as CI, but should get you there most of the way.
	@echo ""

# build the docker image
docker:
	docker build . --tag 'tytanic:latest'

# clean all temporary directories and build artifacts
clean:
	rm -r target
	rm -r {{ book-src / 'build' }}

# install tytanic using cargo
install *args='--force':
	cargo install --path crates/tytanic {{ args }}
