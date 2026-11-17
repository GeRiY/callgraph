<?php

namespace App\Providers;

use App\Events\BookLoaned;
use App\Listeners\SendLoanReceipt;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\ServiceProvider;

class EventServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->app->bind(\App\Services\LoanDesk::class, \App\Services\LoanDesk::class);
    }

    public function boot(): void
    {
        Event::listen(BookLoaned::class, SendLoanReceipt::class);
    }
}
