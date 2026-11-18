--TEST--
Alias loads source
--FILE--
<?php
spl_autoload_register(function($name) { echo $name, "\n"; class Original {} }); var_dump(class_alias("Original", "Another"));
--EXPECT--
Original
bool(true)
