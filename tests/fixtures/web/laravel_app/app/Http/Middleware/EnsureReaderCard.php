<?php

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;

class EnsureReaderCard
{
    public function handle(Request $request, Closure $next)
    {
        if (! $request->user() || $this->exceedsQuota($request)) {
            abort(403);
        }

        return $next($request);
    }

    private function exceedsQuota(Request $request): bool
    {
        return false;
    }
}
