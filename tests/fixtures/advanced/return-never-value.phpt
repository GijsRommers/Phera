--TEST--
return-never-value
--FILE--
<?php
function invalid(): never {
 return 1;
}

--EXPECTF--

Fatal error: A never-returning function must not return in %s on line 3
