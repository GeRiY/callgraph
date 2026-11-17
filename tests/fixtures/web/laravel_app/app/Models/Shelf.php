<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;

class Shelf extends Model
{
    protected $fillable = ['title', 'state', 'stocked_at'];

    public function books()
    {
        return $this->hasMany(Book::class);
    }

    public function scopeStocked(Builder $query): Builder
    {
        return $query->whereNotNull('stocked_at');
    }
}
