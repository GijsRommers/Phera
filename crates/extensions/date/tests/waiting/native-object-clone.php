<?php
$date = new DateTime('2024-01-01 08:09:10.123456 Europe/Brussels');
$copy = clone $date;
$copy->modify('+1 day');
$format = 'Y-m-d\TH:i:s.uP e I';
echo $date->format($format), "\n", $copy->format($format), "\n";
$zone = new DateTimeZone('Asia/Kathmandu');
$zoneCopy = clone $zone;
echo $zoneCopy->getName(), "\n";
$interval = new DateInterval('P1D');
$intervalCopy = clone $interval;
echo $intervalCopy->format('%d'), "\n";
