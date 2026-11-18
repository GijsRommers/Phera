<?php
$interval = DateInterval::__set_state([
    'y' => 0, 'm' => 0, 'd' => 0, 'h' => 0, 'i' => 0, 's' => 0,
    'days' => false,
    'weekday' => 1,
    'weekday_behavior' => 0,
    'have_weekday_relative' => 1,
]);
$date = new DateTimeImmutable('2024-01-30 08:05:01.987654 UTC');
echo $date->add($interval)->format('Y-m-d\TH:i:s.uP e I'), "\n";
try {
    $result = $date->sub($interval);
    echo $result->format('Y-m-d\TH:i:s.uP e I'), "\n";
} catch (DateInvalidOperationException $error) {
    echo $error->getMessage(), "\n";
}
