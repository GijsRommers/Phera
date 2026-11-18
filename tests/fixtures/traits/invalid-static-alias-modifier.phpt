--TEST--
traits: invalid static alias modifier
--FILE--
<?php
trait T { function m() {} }
class C {
    use T {
        m as static;
    }
}
--EXPECTF--
Fatal error: Cannot use "static" as method modifier in trait alias in %s on line 4
