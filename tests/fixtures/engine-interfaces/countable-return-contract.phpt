--TEST--
countable return contract
--FILE--
<?php

class Odd implements Countable {public function count():string { return "4"; }} var_dump(count(new Odd));
--EXPECTF--
Deprecated: Return type of Odd::count(): string should either be compatible with Countable::count(): int, or the #[\ReturnTypeWillChange] attribute should be used to temporarily suppress the notice in %s on line 3
int(4)
