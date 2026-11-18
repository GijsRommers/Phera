<?php
$start = new DateTime('2024-01-31 13:05:08 UTC');
$period = new DatePeriod($start, new DateInterval('P1D'), 2);
$first = $period->start;
$first->modify('+10 days');
echo $period->start->format('Y-m-d\TH:i:s.uP e I'), "\n";
$i = new DateInterval('P1D');
$i->y = '3.5';
$i->f = '0.123456789';
echo json_encode([$i->y, $i->f, $i->from_string]), "\n";
