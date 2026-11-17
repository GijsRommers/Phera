--TEST--
arrow missing capture quiet
--FILE--
<?php
$f=fn()=>isset($missing);var_dump($f());
--EXPECT--
bool(false)
