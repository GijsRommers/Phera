--TEST--
return-never-bare
--FILE--
<?php
function invalid(): never {
 return;
}

--EXPECTF--

Fatal error: A never-returning function must not return in %s on line 3
