<?php
$date = new DateTimeImmutable('2006-12-12 UTC');
$date = $date->setISODate(PHP_INT_MIN, 1, 1);
echo $date->format('Y x X'), "\n";
