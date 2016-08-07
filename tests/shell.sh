SHELL=/bin/sh exec ./testbin -j1 -c 'echo "[$#] [$1] [$2]"' {.} {} ::: a.txt 'b c'.jpg
