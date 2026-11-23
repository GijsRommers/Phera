--TEST--
Early simple class
--FILE--
<?php
var_dump(class_exists("Known", false)); class Known {}
--EXPECT--
bool(true)
