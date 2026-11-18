--TEST--
PHP rejects duplicate get hooks even with different case
--FILE--
<?php
class C {
    public int $x {
        get => 1;
        GET => 2;
    }
}
--EXPECTF--
Fatal error: Cannot redeclare property hook "GET" in %s on line 5
