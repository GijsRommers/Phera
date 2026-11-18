<?php
$date = new DateTimeImmutable("2024-01-31 13:05:08.123456 Europe/Brussels");
$interval = new DateInterval('P2D');
try {
    $period = new DatePeriod($date, $interval, new DateTime("1899-01-01 00:00:00 UTC"), 0);
    echo $period->start->format('Y-m-d\TH:i:s.uP e I'), "\n";
    echo $period->getDateInterval()->format('%d %a'), "\n";
    echo json_encode([
        $period->recurrences, $period->getRecurrences(), $period->include_start_date,
    ]), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
