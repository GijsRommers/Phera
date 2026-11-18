--TEST--
anonymous-classes: same line sites
--FILE--
<?php
$a = new class {}; $b = new class {}; var_dump($a instanceof $b);
--EXPECT--
bool(false)
