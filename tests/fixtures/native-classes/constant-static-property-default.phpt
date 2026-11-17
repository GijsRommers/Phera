--TEST--
Native classes: constant-static-property-default
--FILE--
<?php
class Holder {
 public static $mode=PDO::FETCH_ASSOC;
 }
 var_dump(Holder::$mode);

--EXPECT--
int(2)
