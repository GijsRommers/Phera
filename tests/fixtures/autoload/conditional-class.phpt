--TEST--
Conditional class
--FILE--
<?php
var_dump(class_exists("Known", false)); if (true) { class Known {} } var_dump(class_exists("Known", false));
--EXPECT--
bool(false)
bool(true)
