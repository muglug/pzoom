<?php

// A static property that is only ever written (never read) is NOT reported as
// unused: Psalm counts any static-property access (including a write) as a
// reference. (Instance write-only properties are still reported.)

final class Registry
{
    private static string $last = '';

    public static function set(string $value): void
    {
        self::$last = $value;
    }
}

Registry::set('x');
