--TEST--
A duplicate label is diagnosed before even an invalid backward goto
--FILE--
<?php
while (false) {
    label:
}
goto label;
label:
--EXPECTF--
Fatal error: Label 'label' already defined in %s on line 6
