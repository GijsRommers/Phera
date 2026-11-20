--TEST--
array_multisort reports flag and size errors before publishing anything
--FILE--
<?php
$first = [3, 1];
$second = [1];
foreach ([[SORT_ASC], [777], ['wrong'], [[1], SORT_ASC, SORT_DESC], [[1], SORT_STRING, SORT_NATURAL], [[1], 777]] as $call) {
    try { array_multisort(...$call); }
    catch (Throwable $error) { echo get_class($error), ': ', $error->getMessage(), "\n"; }
}
try { array_multisort($first, SORT_DESC, $second); }
catch (ValueError $error) { echo $error->getMessage(), "\n"; }
echo json_encode([$first, $second]), "\n";
foreach ([
    fn() => array_multisort(array: $first, rest: SORT_DESC),
    fn() => array_multisort(...[$first, 'unknown' => SORT_DESC]),
    fn() => call_user_func_array('array_multisort', [$first, 'unknown' => SORT_DESC]),
    fn() => call_user_func('array_multisort', $first, unknown: SORT_DESC),
] as $call) {
    try { $call(); }
    catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
}
--EXPECT--
TypeError: array_multisort(): Argument #1 ($array) must be an array or a sort flag that has not already been specified
ValueError: array_multisort(): Argument #1 ($array) must be a valid sort flag
TypeError: array_multisort(): Argument #1 ($array) must be an array or a sort flag
TypeError: array_multisort(): Argument #3 must be an array or a sort flag that has not already been specified
TypeError: array_multisort(): Argument #3 must be an array or a sort flag that has not already been specified
ValueError: array_multisort(): Argument #2 must be a valid sort flag
Array sizes are inconsistent
[[3,1],[1]]
array_multisort() does not accept unknown named parameters
array_multisort() does not accept unknown named parameters
array_multisort() does not accept unknown named parameters
array_multisort() does not accept unknown named parameters
