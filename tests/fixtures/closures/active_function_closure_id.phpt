--TEST--
active function closure id
--FILE--
<?php
function g($x){return spl_object_id(new stdClass);}$f=g(...);var_dump($f($f=null));
--EXPECT--
int(2)
