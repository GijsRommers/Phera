--TEST--
increment_string
--FILE--
<?php
$x='az';echo ++$x;
--EXPECTF--

Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead in %s on line 2
ba