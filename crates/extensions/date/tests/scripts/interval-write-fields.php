<?php
$date = new DateTimeImmutable('2024-01-29 12:00:01.777777 UTC');
$span = new DateInterval('P1D');
$span->y = 0;
$span->m = 1;
$span->d = -2;
$span->h = 3;
$span->i = 4;
$span->s = 5;
$span->f = 0.543210;
$span->invert = 1;
echo $span->format('%R %y %m %d %h %i %s %f %a'), "\n";
echo $date->add($span)->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $date->sub($span)->format('Y-m-d\TH:i:s.uP e I'), "\n";
