--TEST--
Function class declaration
--FILE--
<?php
function make() { class Known {} } var_dump(class_exists("Known", false)); make(); var_dump(class_exists("Known", false));
--EXPECT--
bool(false)
bool(true)
