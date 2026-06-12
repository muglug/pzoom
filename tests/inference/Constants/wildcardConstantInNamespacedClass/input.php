<?php
namespace Foo\Bar;

class Scope {
    public const ACTION_END = 'END';
    public const ACTION_BREAK = 'BREAK';
    public const ACTION_NONE = 'NONE';

    /** @return list<self::ACTION_*> */
    public static function getActions(): array {
        return [self::ACTION_END];
    }
}
$actions = Scope::getActions();
$filtered = array_filter($actions, static fn(string $action): bool => $action !== Scope::ACTION_NONE);
