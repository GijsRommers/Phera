--TEST--
PHP forbids hooks on static properties
--FILE--
<?php
class C {
    public static int $x {
        get => 1;
    }
}
--EXPECTF--
Fatal error: Cannot declare hooks for static property in %s on line 4
