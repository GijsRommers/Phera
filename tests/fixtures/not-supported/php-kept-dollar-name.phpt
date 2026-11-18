--TEST--
Named deprecated interpolation compiles once per occurrence
--FILE--
<?php
echo "starting\n";
$name = 'value';
$values = [1 => 'entry'];
for ($i = 0; $i < 2; ++$i) {
    echo "${name} ${values[1]}\n";
}
--EXPECTF--
Deprecated: Using ${var} in strings is deprecated, use {$var} instead in /private%s on line 6

Deprecated: Using ${var} in strings is deprecated, use {$var} instead in /private%s on line 6
starting
value entry
value entry
