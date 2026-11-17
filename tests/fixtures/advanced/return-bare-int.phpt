--TEST--
return-bare-int
--FILE--
<?php
function invalid(): int {
 return;
}

--EXPECTF--

Fatal error: A function with return type must return a value in %s on line 3
