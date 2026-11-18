--TEST--
A tentative native return warning uses the anonymous class display name
--FILE--
<?php
new class implements Countable {
    public function count(): string { return '1'; }
};
--EXPECTF--
Deprecated: Return type of Countable@anonymous::count(): string should either be compatible with Countable::count(): int, or the #[\ReturnTypeWillChange] attribute should be used to temporarily suppress the notice in %s on line 3
