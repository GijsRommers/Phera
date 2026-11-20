--TEST--
array_keys uses loose object comparison for its filter
--EXTENSIONS--
gmp
--FILE--
<?php
$a = new DateTime('2020-01-01');
$b = new DateTimeImmutable('2020-01-01');
echo json_encode(array_keys(['equal' => [$b], 'different' => [$a, $b]], [$a])), "\n";
echo json_encode(array_keys(['equal' => $b], $a, true)), "\n";
set_error_handler(function ($level, $message) { echo $message, "\n"; return true; });
echo json_encode(array_keys(['object' => $a], 1)), "\n";
$five = gmp_init(5);
echo json_encode(array_keys(['low' => gmp_init(2), 'equal' => $five], 5)), "\n";
echo json_encode(array_keys(['equal' => [5]], [$five])), "\n";
--EXPECT--
["equal"]
[]
Object of class DateTime could not be converted to int
["object"]
["equal"]
["equal"]
