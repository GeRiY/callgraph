<?php

namespace App\Http\Requests;

use Illuminate\Foundation\Http\FormRequest;

class StoreShelfRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'title' => ['required', 'string', 'max:120'],
            'stocked_at' => ['nullable', 'date'],
        ];
    }

    public function messages(): array
    {
        return ['title.required' => trans('cards.renewed')];
    }

    public function invalidate(): void
    {
        $this->session()->forget('draft');
    }
}
