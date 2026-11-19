<?php

namespace App\Filament\Resources;

use App\Filament\Resources\PostResource\Pages;
use App\Filament\Resources\PostResource\RelationManagers;
use App\Models\Post;
use Filament\Forms;
use Filament\Forms\Components\DatePicker;
use Filament\Forms\Components\Fieldset;
use Filament\Forms\Components\Grid;
use Filament\Forms\Components\Placeholder;
use Filament\Forms\Components\Section;
use Filament\Forms\Components\Select;
use Filament\Forms\Components\Textarea;
use Filament\Forms\Components\TextInput;
use Filament\Forms\Form;
use Filament\Resources\Resource;
use Filament\Tables;
use Filament\Tables\Table;

class PostResource extends Resource
{
    protected static ?string $model = Post::class;

    protected static ?string $navigationIcon = 'heroicon-o-document-text';

    public static function form(Form $form): Form
    {
        return $form
            ->schema([
                Section::make('Content')
                    ->description('Title, slug and the text')
                    ->schema([
                        Grid::make(2)->schema([
                            TextInput::make('title')->required()->maxLength(255),
                            TextInput::make('slug')->required(),
                        ]),
                        Textarea::make('body')->rows(6),
                        Fieldset::make('Extras')->schema([
                            TextInput::make('tilte'),
                            TextInput::make('reading_time')->disabled()->dehydrated(false),
                        ]),
                    ]),
                Select::make('author_id')
                    ->relationship('author', 'name')
                    ->required()
                    ->searchable(),
                DatePicker::make('published_at'),
                Placeholder::make('hint')->content('Saved as a draft first'),
                Forms\Components\Repeater::make('meta')->schema([
                    TextInput::make('key'),
                    TextInput::make('value'),
                ]),
            ]);
    }

    public static function table(Table $table): Table
    {
        return $table
            ->columns([
                Tables\Columns\TextColumn::make('title')->searchable(),
                Tables\Columns\TextColumn::make('author.name'),
                Tables\Columns\TextColumn::make('status')->badge(),
                Tables\Columns\TextColumn::make('reading_time'),
                Tables\Columns\TextColumn::make('comments_count')->counts('comments'),
                Tables\Columns\TextColumn::make('published_at')->dateTime(),
            ])
            ->filters([
                Tables\Filters\SelectFilter::make('status'),
            ])
            ->actions([
                Tables\Actions\EditAction::make(),
                Tables\Actions\DeleteAction::make(),
                Tables\Actions\Action::make('publish')
                    ->icon('heroicon-o-check')
                    ->action(fn (Post $record) => $record->publish()),
            ])
            ->bulkActions([
                Tables\Actions\BulkActionGroup::make([
                    Tables\Actions\DeleteBulkAction::make(),
                ]),
            ]);
    }

    public static function getRelations(): array
    {
        return [
            RelationManagers\CommentsRelationManager::class,
            RelationManagers\ReviewsRelationManager::class,
        ];
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ListPosts::route('/'),
            'create' => Pages\CreatePost::route('/create'),
            'edit' => Pages\EditPost::route('/{record}/edit'),
        ];
    }
}
