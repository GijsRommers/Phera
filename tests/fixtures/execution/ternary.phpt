--TEST--
ternary
--FILE--
<?php
echo true ? "yes" : "no"; echo 0 ?: 42;
--EXPECT--
yes42