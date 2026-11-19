--TEST--
Autoload follows prototype order and tries every unresolved union member
--FILE--
<?php
spl_autoload_register(function ($class) { echo "autoload $class\n"; });
class D extends DateTime {
    public function diff(DateTimeInterface $t, bool $a = false): Gone|Second {}
    public function getTimezone(): Missing|false { return false; }
}

--EXPECTF--
autoload Missing
autoload Gone
autoload Second

Fatal error: Could not check compatibility between D::getTimezone(): Missing|false and DateTime::getTimezone(): DateTimeZone|false, because class Missing is not available in %s on line 5
