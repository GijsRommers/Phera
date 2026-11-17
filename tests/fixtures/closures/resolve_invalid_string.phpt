--TEST--
resolve invalid string
--FILE--
<?php
$f="missingClosureFixture";try{$f();}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
Call to undefined function missingClosureFixture()
