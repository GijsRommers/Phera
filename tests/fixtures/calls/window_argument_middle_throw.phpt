--TEST--
Call window: argument middle throw
--FILE--
<?php
function f($a,$b,$c){echo 'body';} function mark($s){echo $s;return $s;} function bad(){throw new Exception;} try{f(mark('a'),bad(),mark('c'));}catch(Throwable){echo 'caught';} f(1,2,3);
--EXPECT--
acaughtbody
