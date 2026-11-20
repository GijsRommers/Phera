<?php
declare(strict_types=1);
function tzFromObject(DateTimeImmutable $date): DateTimeZone
{
    return $date->getTimezone();
}
function tzFromInterface(DateTimeInterface $date): DateTimeZone
{
    return $date->getTimezone();
}
