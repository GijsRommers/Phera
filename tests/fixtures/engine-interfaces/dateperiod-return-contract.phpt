--TEST--
dateperiod return contract
--FILE--
<?php
class IncompatiblePeriod extends DatePeriod {
    public function getIterator(): Traversable { return parent::getIterator(); }
}
--EXPECTF--
Fatal error: Declaration of IncompatiblePeriod::getIterator(): Traversable must be compatible with DatePeriod::getIterator(): Iterator in %s on line 3
