--TEST--
Weak exit converts a stringable status into output
--FILE--
<?php
class Text { function __toString() { return 'status'; } } exit(new Text);
--EXPECT--
status
