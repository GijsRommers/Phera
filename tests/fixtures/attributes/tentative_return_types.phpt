--TEST--
ReturnTypeWillChange suppresses tentative native return type diagnostics
--FILE--
<?php
class OrdinaryCounter implements Countable {
    public function count() { return 1; }
}
class TemporaryCounter implements Countable {
    #[ReturnTypeWillChange]
    public function count() { return 2; }
}
echo (new OrdinaryCounter())->count(), " ", (new TemporaryCounter())->count(), "\n";
--EXPECTF--
Deprecated: Return type of OrdinaryCounter::count() should either be compatible with Countable::count(): int, or the #[\ReturnTypeWillChange] attribute should be used to temporarily suppress the notice in %s on line 3
1 2
