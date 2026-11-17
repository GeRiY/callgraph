<?php

namespace App\Enums;

enum LoanState: string
{
    case Open = 'open';
    case Returned = 'returned';
}
