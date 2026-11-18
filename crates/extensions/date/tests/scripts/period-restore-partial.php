<?php
$start = new DateTime('2022-07-14 00:00:00 UTC');
$end = new DateTime('2022-07-16 00:00:00 UTC');
$period = new DatePeriod($start, new DateInterval('P1D'), $end);
try {
    $period->__unserialize([
        'start' => new DateTime('2024-08-28 00:00:00 UTC'),
        'current' => new DateTime('2024-08-27 00:00:00 UTC'),
        'end' => new DateTime('2024-08-29 00:00:00 UTC'),
        'interval' => new DateInterval('P2D'),
        'recurrences' => 2,
        'include_start_date' => 'wrong type',
        'include_end_date' => true,
    ]);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
foreach ([$period->getStartDate(), $period->current, $period->getEndDate()] as $date) {
    echo $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
}
echo $period->getDateInterval()->format('%d'), ' ', $period->getRecurrences(), "\n";
echo json_encode([$period->include_start_date, $period->include_end_date]), "\n";
