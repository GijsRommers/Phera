--TEST--
not_identical_2
--FILE--
<?php
echo (int)(0 !== 1), "\n";
--EXPECT--
1
