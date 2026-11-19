--TEST--
The request precision controls float string conversions
--FILE--
<?php
$number = 1.23456789;
echo $number, ' ', (string) $number, "\n";
ini_set('precision', 4);
echo $number, ' ', (string) $number, "\n";
ini_set('user_agent', $number);
var_dump(ini_get('user_agent'));
ini_restore('precision');
echo $number, "\n";
--EXPECT--
1.23456789 1.23456789
1.235 1.235
string(5) "1.235"
1.23456789
