<?php

namespace App\Http\Resources;

use App\Models\Book;
use Illuminate\Http\Resources\Json\JsonResource;

/**
 * @mixin Book
 */
class BookResource extends JsonResource
{
    public function toArray($request): array
    {
        return [
            'name' => $this->name,
            'cover' => $this->coverUrl(),
        ];
    }
}
