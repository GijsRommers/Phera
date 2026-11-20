--TEST--
array_search uses loose object comparison and keeps the matched key
--EXTENSIONS--
gmp
--FILE--
<?php
$a = new DateTime('2020-01-01');
$b = new DateTimeImmutable('2020-01-01');
var_dump(array_search([$a], ['equal' => [$b]]));
var_dump(array_search($a, ['equal' => $b], true));
set_error_handler(function ($level, $message) { echo $message, "\n"; return true; });
var_dump(array_search(1, ['object' => $a]));
$five = gmp_init(5);
var_dump(array_search(5, ['low' => gmp_init(2), 'equal' => $five]));
var_dump(array_search([$five], ['equal' => [5]]));
--EXPECT--
string(5) "equal"
bool(false)
Object of class DateTime could not be converted to int
string(6) "object"
string(5) "equal"
string(5) "equal"
