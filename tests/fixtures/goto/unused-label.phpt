--TEST--
An unused label in a plain block needs no goto
--FILE--
<?php
{
    unused: echo "ok\n";
}
--EXPECT--
ok
