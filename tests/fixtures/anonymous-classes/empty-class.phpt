--TEST--
anonymous-classes: empty class
--FILE--
<?php
$x = new class {}; echo strstr(get_class($x), chr(0), true);
--EXPECT--
class@anonymous
