DIFF = diff -u

tests = $(patsubst %.sh,%,$(sort $(wildcard *.sh)))

.PHONY: default
default: $(addsuffix .runtest,$(tests))

.PHONY: %.runtest
%.runtest: %.exp %.out %.err
	@$(DIFF) $(basename $@).out $(basename $@).exp
	@if test -f $(basename $@).err_exp ; \
	then $(DIFF) $(basename $@).err_exp $(basename $@).err ; \
	else $(DIFF) /dev/null $(basename $@).err ; \
	fi

%.out %.err: %.sh ../ljobs
	@$(SHELL) $< > $(basename $@).out 2> $(basename $@).err

.INTERMEDIATE: $(addsuffix .out,$(tests))
.INTERMEDIATE: $(addsuffix .err,$(tests))
