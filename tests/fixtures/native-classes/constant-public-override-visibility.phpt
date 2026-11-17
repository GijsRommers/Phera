--TEST--
Native classes: constant-public-override-visibility
--FILE--
<?php
class Child extends PDO {
 protected const int FETCH_ASSOC=9;
 }

--EXPECTF--

Fatal error: Access level to Child::FETCH_ASSOC must be public (as in class PDO) in %s on line 2
