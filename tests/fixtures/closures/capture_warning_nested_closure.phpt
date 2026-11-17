--TEST--
capture warning nested closure
--FILE--
<?php
set_error_handler(function($n,$m){$handler=function()use($m){return strlen($m);};$handler();return true;});$a=4;$f=function()use($a,$missing){return [$a,$missing];};var_dump($f());
--EXPECT--
array(2) {
  [0]=>
  int(4)
  [1]=>
  NULL
}
