--TEST--
PHP rejects an empty hook list
--FILE--
<?php
class C {
    public int $x {}
}
--EXPECTF--
Fatal error: Property hook list must not be empty in %s on line 3
