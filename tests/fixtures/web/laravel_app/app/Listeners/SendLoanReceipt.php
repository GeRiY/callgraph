<?php

namespace App\Listeners;

use App\Events\BookLoaned;

class SendLoanReceipt
{
    public function handle(BookLoaned $event): void
    {
        $this->print($event->loan->reader_ref);
    }

    private function print(string $ref): void
    {
        logger()->info('receipt ' . $ref);
    }
}
