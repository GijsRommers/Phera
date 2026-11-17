<?php
$date = new DateTimeImmutable('2024-01-31 13:05:08.123456 Europe/Brussels');
var_dump($date);
print_r($date);
echo var_export($date, true), "\n";
echo json_encode($date), "\n";
echo json_encode((array) $date), "\n";
$zone = new DateTimeZone('+05:45');
echo json_encode($zone), "\n";
$span = DateInterval::createFromDateString('last day of next month');
echo json_encode((array) $span), "\n";
