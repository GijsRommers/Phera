--TEST--
number_format decimals below the supported integer range
--FILE--
<?php
foreach ([3, 3.5] as $number) {
    try { var_dump(number_format($number, -2147483649)); }
    catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
}
--EXPECT--
number_format(): Argument #2 ($decimals) must be between -2147483648 and 2147483647
number_format(): Argument #2 ($decimals) must be between -2147483648 and 2147483647
