--TEST--
return-bare-nullable
--FILE--
<?php
function invalid(): ?int {
 return;
}

--EXPECTF--

Fatal error: A function with return type must return a value (did you mean "return null;" instead of "return;"?) in %s on line 3
