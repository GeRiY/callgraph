<?php

namespace App\Jobs;

use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Foundation\Bus\Dispatchable;

class BuildOverdueDigest implements ShouldQueue
{
    use Dispatchable;

    public function __construct(public int $shelfNo)
    {
    }

    public function handle(): void
    {
        logger()->info('digest ' . $this->shelfNo);
    }
}
