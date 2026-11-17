<?php

namespace App\Http\Controllers;

use App\Models\Book;

class BookCoverController extends Controller
{
    public function show($query)
    {
        /** @var Book $book */
        $book = $query->first();

        return $book->coverUrl();
    }

    public function index($books)
    {
        $urls = [];
        /** @var Book $b */
        foreach ($books as $b) {
            $urls[] = $b->coverUrl();
        }

        return $urls;
    }
}
