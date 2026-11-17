<?php

use App\Http\Controllers\CatalogueController;

return [
    'pages' => [
        'start' => [
            'uri' => '/',
            'component' => 'Welcome',
            'action' => CatalogueController::class,
        ],
        'colophon' => [
            'uri' => '/colophon',
            'component' => 'Colophon',
            'action' => CatalogueController::class,
        ],
    ],
    'title' => 'Reading Room',
];
