--TEST--
Built-in method compatibility displays its null default
--FILE--
<?php
class D extends DateTime { public static function createFromFormat(int $format, string $datetime, ?DateTimeZone $timezone = null): DateTime|false {} }

--EXPECTF--

Fatal error: Declaration of D::createFromFormat(int $format, string $datetime, ?DateTimeZone $timezone = null): DateTime|false must be compatible with DateTime::createFromFormat(string $format, string $datetime, ?DateTimeZone $timezone = null): DateTime|false in %s on line 2
