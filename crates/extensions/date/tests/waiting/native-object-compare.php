<?php
$a = new DateTime('2024-01-01 00:00:00.000001 UTC');
$b = new DateTimeImmutable('2023-12-31 19:00:00.000001 America/New_York');
$c = new DateTimeImmutable('2024-01-01 00:00:00.000002 UTC');
echo json_encode([$a == $b, $a < $c, $c > $b, $a === $b]), "\n";
echo json_encode([new DateTimeZone('+05:45') == new DateTimeZone('+05:45')]), "\n";
