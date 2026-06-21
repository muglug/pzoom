<?php
class Globals {
    const SUPER_GLOBALS = [
        '$GLOBALS',
        '$_SERVER',
        '$_GET',
    ];

    /** @var array<value-of<self::SUPER_GLOBALS>|'$argv'|'$argc', int> */
    public static array $cache = [];

    public static function test(): void {
        self::$cache['$argv'] = 1;
        self::$cache['$_GET'] = 1;
        self::$cache['$not_a_global'] = 1;
    }
}
