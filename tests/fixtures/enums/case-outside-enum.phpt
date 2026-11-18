--TEST--
enums: case outside enum
--FILE--
<?php
class C {
    case A;
}
--EXPECTF--
Fatal error: Case can only be used in enums in /private%s on line 3
