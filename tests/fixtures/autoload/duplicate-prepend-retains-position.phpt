--TEST--
Duplicate prepend retains position
--FILE--
<?php
function one($name) { echo "one\n"; } function two($name) { echo "two\n"; } spl_autoload_register("one"); spl_autoload_register("two"); spl_autoload_register("one", prepend:true); class_exists("Unknown");
--EXPECT--
one
two
