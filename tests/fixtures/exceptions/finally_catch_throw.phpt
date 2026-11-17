--TEST--
Exceptions: finally_catch_throw
--FILE--
<?php
try { try {throw new Error("x");} catch (Error $error) {throw new Exception("y");} finally {echo "cleanup\n";} } catch(Throwable $error) {echo $error->getMessage();}
--EXPECTF--
cleanup
y