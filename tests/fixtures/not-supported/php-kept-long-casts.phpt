--TEST--
Long cast names warn at compile time before execution or interpolation diagnostics
--FILE--
<?php
echo "starting\n";
$name = 'value';
echo "${name}\n";
var_dump((INTEGER) "7", ( Boolean ) 4, (double) "2.5", (binary) 55);
--EXPECTF--
Deprecated: Non-canonical cast (integer) is deprecated, use the (int) cast instead in /private%s on line 5

Deprecated: Non-canonical cast (boolean) is deprecated, use the (bool) cast instead in /private%s on line 5

Deprecated: Non-canonical cast (double) is deprecated, use the (float) cast instead in /private%s on line 5

Deprecated: Non-canonical cast (binary) is deprecated, use the (string) cast instead in /private%s on line 5

Deprecated: Using ${var} in strings is deprecated, use {$var} instead in /private%s on line 4
starting
value
int(7)
bool(true)
float(2.5)
string(2) "55"
