<?php
$date = new DateTimeImmutable("2024-01-31 13:05:08.123456 Europe/Brussels");
try {
    $modified = $date->modify("2024-02-02 08:00 America/New_York");
        echo $modified->format('Y-m-d\TH:i:s.uP e I'), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
echo $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo json_encode(DateTimeImmutable::getLastErrors()), "\n";
