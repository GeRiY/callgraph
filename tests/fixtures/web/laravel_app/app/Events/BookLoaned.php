<?php

namespace App\Events;

use App\Models\Loan;
use Illuminate\Foundation\Events\Dispatchable;

class BookLoaned
{
    use Dispatchable;

    public function __construct(public Loan $loan, public string $channel)
    {
    }
}
