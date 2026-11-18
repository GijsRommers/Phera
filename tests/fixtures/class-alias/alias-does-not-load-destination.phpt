--TEST--
Alias does not load destination
--FILE--
<?php
class Original {} spl_autoload_register(fn($name) => print($name)); var_dump(class_alias("Original", "Another"));
--EXPECT--
bool(true)
