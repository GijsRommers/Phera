--TEST--
enums: cases ignores ordinary constants
--FILE--
<?php
enum E{case A;const X=self::A;case B;} var_dump(count(E::cases()));
--EXPECT--
int(2)
