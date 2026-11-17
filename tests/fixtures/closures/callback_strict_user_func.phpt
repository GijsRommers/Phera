--TEST--
callback strict user func
--FILE--
<?php
declare(strict_types=1);$f=fn(int $x)=>$x;try{call_user_func($f,"3");}catch(TypeError $e){var_dump(strpos($e->getMessage(),", called in ")!==false);}
--EXPECT--
bool(true)
