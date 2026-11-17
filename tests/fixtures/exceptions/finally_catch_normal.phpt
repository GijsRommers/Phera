--TEST--
Exceptions: finally_catch_normal
--FILE--
<?php
try {throw new Exception("x");} catch(Throwable){echo "catch";} finally {echo "finally";}
--EXPECTF--
catchfinally