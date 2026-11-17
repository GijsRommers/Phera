--TEST--
Exceptions: fatal_final_parent
--FILE--
<?php
final class Base{}try{class Child extends Base{}}catch(Throwable){echo "bad";}
--EXPECTF--

Fatal error: Class Child cannot extend final class Base in %s on line 2
