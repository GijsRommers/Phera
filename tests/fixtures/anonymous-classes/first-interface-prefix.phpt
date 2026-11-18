--TEST--
anonymous-classes: first interface prefix
--FILE--
<?php
interface I {} interface J {} $x = new class implements I,J {}; echo strstr(get_class($x), chr(0), true);
--EXPECT--
I@anonymous
