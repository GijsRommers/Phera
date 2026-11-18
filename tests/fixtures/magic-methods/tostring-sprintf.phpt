--TEST--
Percent-s formats a Stringable object
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
echo sprintf("[%s]", new Text);
--EXPECT--
[hello]
