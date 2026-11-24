<?php
declare(strict_types=1);

class BookingSlot
{
    public function label(): string { return 'slot'; }
}

function requireSlot(BookingSlot $slot): string { return $slot->label(); }

function withAvailableSlot(?BookingSlot $slot): ?string
{
    $label = $slot === null ? null : $slot->label();
    return $label === null ? null : requireSlot($slot);
}
