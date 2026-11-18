--TEST--
Trait aliases cannot declare readonly methods
--FILE--
<?php
trait T { function m() {} }
class C {
    use T {
        m as
        readonly;
    }
}
--EXPECTF--
Fatal error: Cannot use the readonly modifier on a method in %s on line 6
