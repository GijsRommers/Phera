--TEST--
Overflow notices during radix conversion
--FILE--
<?php
set_error_handler(function ($severity, $message) { echo $severity, ': ', $message, PHP_EOL; });
foreach ([
    fn() => bindec(str_repeat('1', 64)),
    fn() => octdec(str_repeat('7', 24)),
    fn() => hexdec(str_repeat('f', 16) . 'x'),
    fn() => base_convert(str_repeat('f', 16), 16, 10),
] as $convert) {
    var_dump($convert());
}
--EXPECT--
8: Input number is larger than PHP_INT_MAX, precision has been lost in conversion
float(1.8446744073709552E+19)
8: Input number is larger than PHP_INT_MAX, precision has been lost in conversion
float(4.722366482869645E+21)
8: Input number is larger than PHP_INT_MAX, precision has been lost in conversion
8192: Invalid characters passed for attempted conversion, these have been ignored
float(1.8446744073709552E+19)
8: Input number is larger than PHP_INT_MAX, precision has been lost in conversion
string(20) "18446744073709552046"
