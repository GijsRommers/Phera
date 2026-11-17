<?php
try {
    $period = DatePeriod::createFromISO8601String("20240101T000000Z/P2D/20240105T000000Z", 2);
    echo $period->getStartDate()->format('Y-m-d\TH:i:s.uP e I'), "\n";
    echo $period->getDateInterval()->format('%d %s %a'), "\n";
    echo json_encode([$period->recurrences, $period->getRecurrences(), $period->current]), "\n";
    echo json_encode([$period->include_start_date, $period->include_end_date]), "\n";
    if ($period->getEndDate() !== null) {
        echo $period->getEndDate()->format('Y-m-d\TH:i:s.uP e I'), "\n";
    }
} catch (Throwable $error) {
    echo get_class($error), " ", $error->getMessage(), "\n";
}
