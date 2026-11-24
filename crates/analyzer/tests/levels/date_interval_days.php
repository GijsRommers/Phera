<?php
declare(strict_types=1);
function diffDays(DateTimeImmutable $start, DateTimeInterface $end): int
{
    return $start->diff($end)->days;
}
function assignedDays(DateTimeImmutable $start, DateTimeInterface $end): int
{
    $interval = $start->diff($end);
    return $interval->days;
}
function anyDays(DateInterval $interval): int
{
    return $interval->days;
}
