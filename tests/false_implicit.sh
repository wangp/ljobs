./testbin -j1 echo '{}' ::: '{}'
./testbin -j1 echo '{.}' ::: '{.}.'
# {/}
./testbin -j1 echo '{//}' ::: '{//}/x'
# {/.}
# {#}
