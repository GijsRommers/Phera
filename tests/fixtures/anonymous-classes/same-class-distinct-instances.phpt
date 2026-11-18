--TEST--
anonymous-classes: same class distinct instances
--FILE--
<?php
function make() { return new class {}; } var_dump(make() === make());
--EXPECT--
bool(false)
