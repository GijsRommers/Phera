--TEST--
invoke not callable
--FILE--
<?php
$f=new stdClass;var_dump(is_callable($f));try{$f();}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
bool(false)
Object of type stdClass is not callable
