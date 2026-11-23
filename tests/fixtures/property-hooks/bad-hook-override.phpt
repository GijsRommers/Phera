--TEST--
#[Override] on a hook without a corresponding parent declaration
--FILE--
<?php
class Invalid {
    public int $x {
        #[Override]
        get => 2;
    }
}
--EXPECTF--

Fatal error: Invalid::$x::get() has #[\Override] attribute, but no matching parent method exists in %s on line 5
