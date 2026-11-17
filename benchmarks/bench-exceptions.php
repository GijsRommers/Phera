<?php
// One million calls exercise rare throws and a shared finally body.
function exception_work($iteration)
{
    if ($iteration % 10 === 0) {
        throw new RuntimeException('retry');
    }
    return 1;
}

$completed = 0;
$caught = 0;
$cleaned = 0;
for ($iteration = 0; $iteration < 1000000; $iteration++) {
    try {
        $completed += exception_work($iteration);
    } catch (RuntimeException $failure) {
        $caught++;
    } finally {
        $cleaned++;
    }
}
echo $completed, '|', $caught, '|', $cleaned, "\n";
