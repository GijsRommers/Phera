--TEST--
closure invoke exists
--FILE--
<?php
$f=fn()=>1;var_dump(method_exists($f,"__invoke"),method_exists(Closure::class,"__invoke"));
--EXPECT--
bool(true)
bool(true)
