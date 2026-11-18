<?php
$date = new DateTimeImmutable("2024-01-31 13:05:08.123456 Europe/Brussels");
$result = $date->setTimestamp(-123456789);
echo $result->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $result->getTimestamp(), ' ', $result->getMicrosecond(), ' ', $result->getOffset(), "\n";
echo $result->getTimezone()->getName(), "\n";
echo $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
