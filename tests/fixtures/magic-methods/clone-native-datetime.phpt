--TEST--
Cloning a DateTime preserves and independently copies native state
--FILE--
<?php
$original = new DateTime('2024-01-01T08:09:10.123456 Europe/Brussels');
$copy = clone $original;
$copy->modify('+1 day');
echo $original->format('d.u e'), '/', $copy->format('d.u e');
--EXPECT--
01.123456 Europe/Brussels/02.123456 Europe/Brussels
