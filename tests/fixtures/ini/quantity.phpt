--TEST--
INI quantities support powers of two, base prefixes and signed values
--FILE--
<?php
foreach (['4K', '2M', '-2G', '0x12', '0b11', '077', '0o11', ' 1 k ', '0', ''] as $value) {
    var_dump(ini_parse_quantity($value));
}
--EXPECT--
int(4096)
int(2097152)
int(-2147483648)
int(18)
int(3)
int(63)
int(9)
int(1024)
int(0)
int(0)
