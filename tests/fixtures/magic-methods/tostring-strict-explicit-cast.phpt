--TEST--
tostring strict explicit cast
--FILE--
<?php
declare(strict_types=1); class Text { function __toString() { return "hello"; } }
echo (string) new Text;
--EXPECT--
hello
