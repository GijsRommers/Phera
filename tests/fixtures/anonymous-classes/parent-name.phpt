--TEST--
anonymous-classes: parent name
--FILE--
<?php
class P {} $x = new class extends P {}; echo strstr(get_class($x), chr(0), true);
--EXPECT--
P@anonymous
