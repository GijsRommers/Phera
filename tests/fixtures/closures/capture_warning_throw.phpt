--TEST--
capture warning throw
--FILE--
<?php
set_error_handler(function($n,$m){throw new RuntimeException("capture");});try{$a=4;$f=function()use($a,$missing){};}catch(RuntimeException $e){echo $e->getMessage(),"
";}restore_error_handler();$g=fn()=>3;var_dump($g());
--EXPECT--
capture
int(3)
