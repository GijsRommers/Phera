<?php
class FactoryDate extends DateTimeImmutable {}
class FactoryPeriod extends DatePeriod {}
$stamp = FactoryDate::createFromTimestamp(0);
$zone = new DateTimeZone('Europe/Brussels');
$parsed = FactoryDate::createFromFormat('!Y-m-d', '2000-01-01', $zone);
$copy = FactoryDate::createFromInterface($parsed);
$period = FactoryPeriod::createFromISO8601String('R2/20240101T000000Z/P1D');
foreach ([$stamp, $parsed, $copy, $period] as $value) {
    echo get_class($value), "\n";
}
