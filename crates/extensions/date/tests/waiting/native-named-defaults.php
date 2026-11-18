<?php
$date = new DateTimeImmutable('2024-01-31 UTC');
echo $date->setTime(hour: 1, minute: 2, microsecond: 99)->format('Y-m-d\TH:i:s.uP e I'), "\n";
$zone = new DateTimeZone('Europe/Brussels');
echo $zone->getTransitions(timestampEnd: -2208988800)[0]['ts'], "\n";
echo date_time_set(new DateTime('2024-01-31 UTC'), 1, 2, microsecond: 99)
    ->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo timezone_transitions_get($zone, timestampEnd: -2208988800)[0]['ts'], "\n";
echo count(timezone_identifiers_list(countryCode: 'BE')), "\n";
echo timezone_name_from_abbr('', isDST: 0) === false ? "false\n" : "found\n";
echo strlen(date_create(timezone: $zone)->format('u')), "\n";
echo strlen(date_create_immutable(timezone: $zone)->format('u')), "\n";
