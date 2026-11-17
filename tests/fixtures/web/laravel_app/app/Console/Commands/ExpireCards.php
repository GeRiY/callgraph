<?php

namespace App\Console\Commands;

use Illuminate\Console\Command;

class ExpireCards extends Command
{
    protected $signature = 'cards:expire-reader {number : The card to expire}';

    public function handle(): int
    {
        $this->info('expired');

        return 0;
    }
}
