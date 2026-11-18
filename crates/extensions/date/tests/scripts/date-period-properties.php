<?php
function inspect_period_properties($period) {
    foreach (['start', 'current', 'end', 'interval', 'recurrences',
        'include_start_date', 'include_end_date'] as $name) {
        $value = $period->$name;
        if ($value instanceof DateTimeInterface) {
            $value = $value->format('Y-m-d\TH:i:s.uP e I');
        } elseif ($value instanceof DateInterval) {
            $value = $value->format('%R%a %d');
        }
        echo json_encode([
            $name, $value, isset($period->$name), property_exists($period, $name),
            property_exists(DatePeriod::class, $name),
        ]), "\n";
        try {
            $period->$name = 'changed';
        } catch (Error $error) {
            echo $error->getMessage(), "\n";
        }
        try {
            unset($period->$name);
        } catch (Error $error) {
            echo $error->getMessage(), "\n";
        }
    }
}
$start = new DateTime('2024-01-01 08:00:00 UTC');
$interval = new DateInterval('P1D');
$period = new DatePeriod($start, $interval, 3);
inspect_period_properties($period);
inspect_period_properties(new DatePeriod($start, $interval, new DateTime('2024-01-03 UTC'), 3));
class UninitializedPeriod extends DatePeriod { public function __construct() {} }
$uninitialized = new UninitializedPeriod();
inspect_period_properties($uninitialized);
echo json_encode([
    $uninitialized->recurrences ?? 'missing', empty($uninitialized->recurrences),
    $uninitialized->include_start_date ?? 'missing',
]), "\n";
$first = $period->start;
$first->modify('+10 days');
echo $period->start->format('Y-m-d\TH:i:s.uP e I'), "\n";
$firstInterval = $period->interval;
$firstInterval->d = 15;
echo $period->interval->format('%d'), "\n";
try {
    $reference =& $period->interval;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
