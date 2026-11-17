--TEST--
Exceptions: catch_unknown
--FILE--
<?php
try { throw new Exception("x"); } catch (UnknownClass) {echo "bad";} catch(Throwable) {echo "ok";}
--EXPECTF--
ok