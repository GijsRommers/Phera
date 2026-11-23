--TEST--
anonymous-classes: parent prefix beats interface
--FILE--
<?php
class P {} interface I {} $x = new class extends P implements I {}; echo strstr(get_class($x), chr(0), true);
--EXPECT--
P@anonymous
