<?php
set_error_handler(function ($level, $message) {
    echo $level, ' ', $message, "\n";
    return true;
});
$date = date_create('2024-01-31 11:22:33 UTC');
echo json_encode(date_create('bad-zone')), "\n";
echo json_encode(date_get_last_errors()), "\n";
echo json_encode(date_modify($date, 'bad-zone')), "\n";
echo json_encode(date_interval_create_from_date_string('2024-01-31')), "\n";
echo json_encode(timezone_open('Not/AZone')), "\n";
$unchanged = date_sub($date, DateInterval::createFromDateString('next monday'));
echo $unchanged->format('Y-m-d\TH:i:s.uP e I'), "\n";
