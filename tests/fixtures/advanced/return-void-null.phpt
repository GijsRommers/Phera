--TEST--
return-void-null
--FILE--
<?php
function invalid(): void {
 return null;
}

--EXPECTF--

Fatal error: A void function must not return a value (did you mean "return;" instead of "return null;"?) in %s on line 3
