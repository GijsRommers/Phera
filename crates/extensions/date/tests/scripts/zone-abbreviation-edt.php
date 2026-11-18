<?php
$zone = new DateTimeZone("EdT");
$date = DateTimeImmutable::createFromTimestamp(1704067200);
echo $zone->getName(), ' ', $zone->getOffset($date), "\n";
echo timezone_offset_get($zone, $date), "\n";
$date = $date->setTimezone($zone);
echo $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
