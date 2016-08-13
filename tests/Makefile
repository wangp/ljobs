tests = $(patsubst %.sh,%,$(sort $(wildcard *.sh)))

.PHONY: default
default: $(addsuffix .runtest,$(tests))

.PHONY: %.runtest
%.runtest: %.sh
	@$(SHELL) ./runtest $<
