--TEST--
A return type against a built-in method that names a class nobody can load is a fatal error, not a deprecation
--FILE--
<?php
spl_autoload_register(function ($class) { echo "autoload $class\n"; });
class D extends DateTime {
    public function getTimezone(): Missing|false { return false; }
}
echo "declared\n";
--EXPECTF--
autoload Missing

Fatal error: Could not check compatibility between D::getTimezone(): Missing|false and DateTime::getTimezone(): DateTimeZone|false, because class Missing is not available in %s on line %d
