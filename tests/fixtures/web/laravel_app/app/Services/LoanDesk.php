<?php

namespace App\Services;

use App\Events\BookLoaned;
use App\Models\Loan;

class LoanDesk
{
    public function checkout(array $data, string $channel): Loan
    {
        $loan = new Loan($data);
        $loan->save();

        BookLoaned::dispatch($loan, $channel);

        return $loan;
    }

    public function describe(array $data): array
    {
        $options = config('desk.options');
        $response = response()->json($data);
        $copy = clone $response;

        return [$options, $response->getStatusCode(), $this->configure($data), $this->condone($copy)];
    }

    private function configure(array $data): array
    {
        return array_filter($data);
    }

    private function condone(mixed $value): mixed
    {
        return $value;
    }
}
