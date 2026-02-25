define confirm_changelog
	@echo "Did you update CHANGELOG.md? [y/N]" && read ans && [ "$$ans" = "y" ] || (echo "Aborting." && exit 1)
endef

.PHONY: release release-minor release-major

release:
	$(confirm_changelog)
	cargo release patch --no-publish --execute

release-minor:
	$(confirm_changelog)
	cargo release minor --no-publish --execute

release-major:
	$(confirm_changelog)
	cargo release major --no-publish --execute
