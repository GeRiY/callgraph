<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;
use Illuminate\Database\Eloquent\Relations\HasMany;

class Post extends Model
{
    use HasFactory;

    protected $fillable = ['title', 'slug', 'body', 'author_id', 'status', 'published_at', 'meta'];

    protected $casts = [
        'published_at' => 'datetime',
        'meta' => 'array',
    ];

    public function author(): BelongsTo
    {
        return $this->belongsTo(User::class, 'author_id');
    }

    public function comments(): HasMany
    {
        return $this->hasMany(Comment::class);
    }

    public function getReadingTimeAttribute(): int
    {
        return max(1, intdiv(str_word_count((string) $this->body), 200));
    }

    public function publish(): void
    {
        $this->update(['status' => 'published', 'published_at' => now()]);
    }
}
