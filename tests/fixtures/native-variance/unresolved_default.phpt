--TEST--
Unresolved DateTime diff return reports the built-in default
--FILE--
<?php
spl_autoload_register(function ($c) { echo "autoload $c\n"; });
class D extends DateTime {
    #[\ReturnTypeWillChange]
    public function diff(DateTimeInterface $t, bool $a = false): Gone { }
}
echo "declared\n";

--EXPECTF--
autoload Gone

Fatal error: Could not check compatibility between D::diff(DateTimeInterface $t, bool $a = false): Gone and DateTime::diff(DateTimeInterface $targetObject, bool $absolute = false): DateInterval, because class Gone is not available in %s on line 5
