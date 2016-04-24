.PHONY: check
check: ljobs
	L --norun --warn-undefined-fns ljobs

.PHONY: test
test:
	@$(MAKE) -C tests
