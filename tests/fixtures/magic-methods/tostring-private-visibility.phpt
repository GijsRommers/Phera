--TEST--
A private __toString warns and violates Stringable visibility
--FILE--
<?php
class Text {
    private function __toString() { return "x"; }
}
--EXPECTF--
Warning: The magic method Text::__toString() must have public visibility in %s on line 3

Fatal error: Access level to Text::__toString() must be public (as in class Stringable) in %s on line 3
