SHELL=/bin/sh exec ../ljobs -j1 -c 'echo "[$#] [$1] [$2]"' {.} {} ::: a.txt 'b c'.jpg
