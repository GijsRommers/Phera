<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

/**
 * A model whose boot asks for the signed-in user, as a tenant-scoped model
 * of a real application might. It can only boot inside a request.
 */
class Note extends Model
{
    /** How often booted() ran in this process, the warm-up included. */
    public static int $boots = 0;

    protected static function booted(): void
    {
        self::$boots++;
        static::creating(fn () => null);
        static::addGlobalScope('visible', fn ($query) => $query->where('visible', true));
        $owner = auth()->user();
        static::addGlobalScope('owner', fn ($query) => $query->where('owner_id', $owner?->id));
    }
}
