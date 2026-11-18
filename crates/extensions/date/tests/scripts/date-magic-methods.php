<?php
$date = new DateTimeImmutable('2024-05-30 12:00:15.543210 Pacific/Chatham');
$data = $date->__serialize();
$restored = DateTimeImmutable::__set_state($data);
$other = new DateTimeImmutable('2000-01-01 UTC');
$other->__unserialize($data);
echo json_encode($data), "\n";
echo $restored->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $other->format('Y-m-d\TH:i:s.uP e I'), "\n";

$zone = new DateTimeZone('-02:15:31');
$zoneData = $zone->__serialize();
$restoredZone = DateTimeZone::__set_state($zoneData);
$zone->__unserialize(['timezone_type' => 2, 'timezone' => 'edt']);
echo json_encode([$zoneData, $restoredZone->getName(), $zone->getName()]), "\n";

$interval = DateInterval::createFromDateString('2 weekdays');
$intervalData = $interval->__serialize();
echo json_encode($intervalData), "\n";
echo DateInterval::__set_state($intervalData)->format('%d %a'), "\n";

$period = new DatePeriod($date, $interval, 3);
$restoredPeriod = DatePeriod::__set_state($period->__serialize());
echo $restoredPeriod->getRecurrences(), "\n";
echo $restoredPeriod->getStartDate()->format('Y-m-d\TH:i:s.uP e I'), "\n";
