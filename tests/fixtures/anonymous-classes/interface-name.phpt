--TEST--
anonymous-classes: interface name
--FILE--
<?php
interface I {} $x = new class implements I {}; echo strstr(get_class($x), chr(0), true);
--EXPECT--
I@anonymous
