--TEST--
foreach_destructure
--FILE--
<?php
foreach([[1,2],[3,4]] as [$a,$b])echo $a,$b;
--EXPECT--
1234