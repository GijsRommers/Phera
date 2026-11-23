<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

/**
 * A model whose boot succeeds and keeps something of the moment it ran: it
 * reads the clock, not the request. Under PHP that moment is in every
 * request; with Phera's warm-up it is once per worker.
 */
class Stamp extends Model
{
    public static float $bootedAt = 0.0;

    protected static function booted(): void
    {
        self::$bootedAt = microtime(true);
    }
}
