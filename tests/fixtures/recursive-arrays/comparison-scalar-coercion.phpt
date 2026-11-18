--TEST--
Flat arrays compare numbers to nonnumeric strings using PHP number text
--FILE--
<?php
var_dump([-9223372036854775807 - 1] <=> ["-9223372036854775808x"], [1e25] <=> ["1.0E+25x"], [0.0001] <=> ["0.0001x"], [-0.0] <=> ["-0x"]);
--EXPECT--
int(-1)
int(-1)
int(-1)
int(-1)
