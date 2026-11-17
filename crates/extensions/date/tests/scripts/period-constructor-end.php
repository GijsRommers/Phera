<?php
$date = new DateTimeImmutable("2024-01-31 13:05:08.123456 Europe/Brussels");
$interval = new DateInterval('P2D');
try {
    $end = new DateTime("2024-02-05 14:05:00 Europe/Brussels");
    $period = new DatePeriod($date, $interval, $end, 0);
    echo $period->start->format('Y-m-d\TH:i:s.uP e I'), "\n";
    echo $period->getDateInterval()->format('%d %a'), "\n";
    echo json_encode([
        $period->recurrences, $period->getRecurrences(), $period->include_start_date,
    ]), "\n";
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
