--TEST--
not_identical_1
--FILE--
<?php
echo (int)(-7 !== 3), "\n";
--EXPECT--
1
