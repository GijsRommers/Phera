--TEST--
Class members: static-missing-isset
--FILE--
<?php class Foo {} var_dump(isset(Foo::$x));
--EXPECT--
bool(false)
