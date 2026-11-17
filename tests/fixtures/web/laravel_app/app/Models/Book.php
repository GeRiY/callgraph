<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class Book extends Model
{
    protected $fillable = ['name', 'shelf_no'];

    public function event()
    {
        return $this->belongsTo(Event::class);
    }
}
