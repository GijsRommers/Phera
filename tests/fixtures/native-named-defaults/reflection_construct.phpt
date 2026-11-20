--TEST--
Native named defaults: reflection construct
--FILE--
<?php
$tz = new DateTimeZone('Asia/Tokyo');
$r = new ReflectionClass(DateTimeImmutable::class);
echo $r->newInstanceArgs(['timezone' => $tz])->getTimezone()->getName(), "\n";
?>
--EXPECT--
Asia/Tokyo
