<?php
$date = new DateTimeImmutable('2024-01-31 13:05:08.123456 Europe/Brussels');
$serialized = serialize($date);
echo $serialized, "\n";
echo unserialize($serialized)->format('Y-m-d\TH:i:s.uP e I'), "\n";
$span = DateInterval::createFromDateString('next Monday');
echo serialize($span), "\n";
echo unserialize(serialize($span))->format('%R%d %a'), "\n";
