exec ./testbin -j1 echo '[{}] => [{//}]' <<EOF

.
/
//
/.
/..
./
../
/a
/a/b
//a/b
//a/b/
/a/b///
a/
a/b
a/b//c
a:/
a:/b
EOF
