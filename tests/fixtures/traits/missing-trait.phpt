--TEST--
traits: missing trait
--FILE--
<?php
try{class C{use Missing;}}catch(Error $e){echo $e->getMessage();}
--EXPECT--
Trait "Missing" not found
