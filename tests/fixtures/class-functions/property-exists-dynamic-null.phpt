--TEST--
Property exists dynamic null
--FILE--
<?php
$x=new stdClass; $x->p=null; var_dump(property_exists($x,"p"));
--EXPECT--
bool(true)
