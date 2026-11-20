--TEST--
Native named defaults: instance method
--FILE--
<?php
$d = new DateTimeImmutable('2020-01-01 12:34:56+00:00');
echo $d->setTime(1, 2, microsecond: 654321)->format('H:i:s.u'), "\n";
$r = new ReflectionMethod(DateTimeImmutable::class, 'setTime');
echo $r->invokeArgs($d, [1, 2, 'microsecond' => 654321])->format('H:i:s.u'), "\n";
?>
--EXPECT--
01:02:00.654321
01:02:00.654321
