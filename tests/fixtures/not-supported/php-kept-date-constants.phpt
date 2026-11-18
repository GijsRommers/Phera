--TEST--
Retained date constants warn on reads including on subclasses
--FILE--
<?php
var_dump(defined('DATE_RFC7231'), defined('DateTimeInterface::RFC7231'));
var_dump(defined('DateTimeImmutable::RFC7231'));
var_dump(DATE_RFC7231, DateTimeInterface::RFC7231, DateTimeImmutable::RFC7231);
var_dump(constant('DateTime::RFC7231'));
--EXPECTF--
bool(true)
bool(true)
bool(true)

Deprecated: Constant DATE_RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT in %s on line 4

Deprecated: Constant DateTimeInterface::RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT in %s on line 4

Deprecated: Constant DateTimeInterface::RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT in %s on line 4
string(21) "D, d M Y H:i:s \G\M\T"
string(21) "D, d M Y H:i:s \G\M\T"
string(21) "D, d M Y H:i:s \G\M\T"

Deprecated: Constant DateTimeInterface::RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT in %s on line 5
string(21) "D, d M Y H:i:s \G\M\T"
