--TEST--
A class import conflicts with a prior declaration even in an unexecuted branch
--FILE--
<?php
if (false) {
    class AliasName {}
}
use Other\Target as AliasName;
--EXPECTF--
Fatal error: Cannot use Other\Target as AliasName because the name is already in use in %s on line 5
