<?php
class C {
    /** @var array<string, string>|null */
    private static $map = [];

    public static function foo(string $id) : ?string {
        if (isset(self::$map[$id])) {
            return self::$map[$id];
        }

        return null;
    }
}