--TEST--
first class dynamic function
--FILE--
<?php
$name="strlen";$g=$name(...);var_dump($g instanceof Closure,$g("abc"));
--EXPECT--
bool(true)
int(3)
