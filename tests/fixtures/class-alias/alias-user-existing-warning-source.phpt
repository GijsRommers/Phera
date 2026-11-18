--TEST--
Alias user existing warning source
--FILE--
<?php
class Original {} class Other {} var_dump(class_alias("Original", "Other"));
--EXPECTF--

Warning: Cannot redeclare class Other (previously declared in %s/alias-user-existing-warning-source.php:2) in %s/alias-user-existing-warning-source.php on line 2
bool(false)
