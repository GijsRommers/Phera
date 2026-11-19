--TEST--
number_format decimals below the supported integer range
--FILE--
<?php
foreach ([3, 3.5] as $number) {
    try { var_dump(number_format($number, -2147483649)); }
    catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
}
--EXPECT--
string(1) "0"
string(1) "0"
