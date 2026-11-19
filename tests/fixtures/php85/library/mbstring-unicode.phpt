--TEST--
PHP Unicode tables control case conversion, folded search and display width
--FILE--
<?php
$lower = mb_chr(0x0277, 'UTF-8');
$upper = mb_chr(0xa7dd, 'UTF-8');
$wide = mb_chr(0x18cd6, 'UTF-8');
var_dump(
    mb_ord(mb_strtoupper($lower, 'UTF-8'), 'UTF-8'),
    mb_ord(mb_strtolower($upper, 'UTF-8'), 'UTF-8'),
    mb_stripos('a' . $upper, $lower, 0, 'UTF-8'),
    mb_strwidth($wide, 'UTF-8'),
    bin2hex(mb_strimwidth($wide . 'x', 0, 2, '.', 'UTF-8')),
);
--EXPECT--
int(631)
int(42973)
bool(false)
int(1)
string(10) "f098b39678"
