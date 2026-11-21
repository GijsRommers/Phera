--TEST--
Declaring an interface named underscore retains PHP’s deprecation
--FILE--
<?php
interface _ {}
var_dump(interface_exists('_'));
--EXPECTF--
Deprecated: Using "_" as an interface name is deprecated since 8.4 in %s on line 2
bool(true)
