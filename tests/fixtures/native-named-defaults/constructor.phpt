--TEST--
Native named defaults: constructor
--FILE--
<?php
$tz = new DateTimeZone('Asia/Tokyo');
$d = new DateTimeImmutable(timezone: $tz);
echo $d->getTimezone()->getName(), "\n";
?>
--EXPECT--
Asia/Tokyo
