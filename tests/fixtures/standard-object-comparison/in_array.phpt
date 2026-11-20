--TEST--
in_array uses loose object comparison and preserves strict identity
--EXTENSIONS--
gmp
--FILE--
<?php
$a = new DateTime('2020-01-01');
$b = new DateTimeImmutable('2020-01-01');
var_dump(in_array($a, [$b]), in_array([$a], [[$b]]), in_array($a, [$b], true));
set_error_handler(function ($level, $message) { echo $message, "\n"; return true; });
var_dump(in_array($a, [1]), in_array(1, [$a]));
$five = gmp_init(5);
var_dump(in_array($five, [5]), in_array(5, [$five]), in_array([$five], [[5]]));
var_dump(in_array(2, [$five]), in_array($five, [5], true));
--EXPECT--
bool(true)
bool(true)
bool(false)
Object of class DateTime could not be converted to int
Object of class DateTime could not be converted to int
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
