--TEST--
Try all deferred built-in return type names before reporting the first unresolved method
--FILE--
<?php
spl_autoload_register(function ($c) { echo "autoload $c\n"; });
class D extends DateTime {
    public function getTimezone(): Missing|false { return false; }
    #[\ReturnTypeWillChange]
    public function diff(DateTimeInterface $t, bool $a = false): Gone { }
}
echo "declared\n";

--EXPECTF--
autoload Missing
autoload Gone

Fatal error: Could not check compatibility between D::getTimezone(): Missing|false and DateTime::getTimezone(): DateTimeZone|false, because class Missing is not available in %s on line 4
