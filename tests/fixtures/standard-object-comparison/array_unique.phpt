--TEST--
array_unique SORT_REGULAR uses object comparisons in both sorting and grouping
--EXTENSIONS--
gmp
--FILE--
<?php
$a = new DateTime('2020-01-01');
$b = new DateTimeImmutable('2020-01-01');
$c = new DateTime('2021-01-01');
echo json_encode(array_keys(array_unique(['a' => [$a], 'c' => [$c], 'b' => [$b]], SORT_REGULAR))), "\n";
set_error_handler(function ($level, $message) { echo $message, "\n"; return true; });
echo json_encode(array_keys(array_unique(['object' => $a, 'scalar' => 1], SORT_REGULAR))), "\n";
$five = gmp_init(5);
echo json_encode(array_keys(array_unique(['gmp' => $five, 'int' => 5, 'low' => 2], SORT_REGULAR))), "\n";
echo json_encode(array_keys(array_unique(['gmp' => [$five], 'int' => [5]], SORT_REGULAR))), "\n";
--EXPECT--
["a","c"]
Object of class DateTime could not be converted to int
Object of class DateTime could not be converted to int
["object"]
["gmp","low"]
["gmp"]
