--TEST--
Late implements known interface
--FILE--
<?php
var_dump(class_exists("Known", false)); interface Face {} class Known implements Face {}
--EXPECT--
bool(false)
