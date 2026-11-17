--TEST--
Exceptions: catch_nested
--FILE--
<?php
try { try { throw new Error("x"); } catch(Exception) {echo "bad";} } catch(Throwable $error) { echo $error->getMessage(); }
--EXPECTF--
x