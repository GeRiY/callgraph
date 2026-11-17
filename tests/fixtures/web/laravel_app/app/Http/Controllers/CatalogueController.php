<?php

namespace App\Http\Controllers;

class CatalogueController extends Controller
{
    public function __invoke()
    {
        return inertia('Catalogue');
    }
}
