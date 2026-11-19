--TEST--
A duplicate label reports the line of the second identifier
--FILE--
<?php
here:
{
    here
        :
}
--EXPECTF--
Fatal error: Label 'here' already defined in %s on line 4
