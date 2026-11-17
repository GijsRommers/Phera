--TEST--
Native classes: constant-case-sensitive
--FILE--
<?php
try {
 var_dump(PDO::fetch_assoc);
 }
 catch (Error $e) {
 echo $e->getMessage(), "\n";
 }

--EXPECT--
Undefined constant PDO::fetch_assoc
