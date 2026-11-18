--TEST--
anonymous-classes: separate sites
--FILE--
<?php
var_dump(get_class(new class {}) === get_class(new class {}));
--EXPECT--
bool(false)
