--TEST--
anonymous-classes: repeated site
--FILE--
<?php
function make() { return new class {}; } var_dump(get_class(make()) === get_class(make()));
--EXPECT--
bool(true)
