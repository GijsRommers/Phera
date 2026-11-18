--TEST--
countable private method
--FILE--
<?php
class PrivateCount implements Countable
{
    private function count(): int
    {
        return 1;
    }
}
--EXPECTF--
Fatal error: Access level to PrivateCount::count() must be public (as in class Countable) in %s on line 4
