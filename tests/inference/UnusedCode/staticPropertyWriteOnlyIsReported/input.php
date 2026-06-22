<?php

// A static property that is only ever written (never read) is genuine dead
// storage. pzoom deliberately reports it (a useful divergence from Psalm, which
// treats any static-property access as a use and stays silent).

final class Registry
{
    private static string $last = '';

    public static function set(string $value): void
    {
        self::$last = $value;
    }
}

Registry::set('x');
