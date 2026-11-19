<?php

namespace App\Providers;

use Illuminate\Support\Facades\Blade;
use Illuminate\Support\ServiceProvider;

class AppServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        Blade::directive('datetime', fn ($expression) => "<?php echo ($expression)->format('Y-m-d'); ?>");
    }
}
