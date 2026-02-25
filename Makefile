.PHONY: release release-minor release-major

release:
	cargo release patch --execute

release-minor:
	cargo release minor --execute

release-major:
	cargo release major --execute
