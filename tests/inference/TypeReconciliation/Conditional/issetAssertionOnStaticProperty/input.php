<?php
class C {
    protected static array $cache = [];

    /**
     * @psalm-suppress MixedReturnStatement
     */
    public static function get(string $k1, string $k2) : ?string {
        if (!isset(static::$cache[$k1][$k2])) {
            return null;
        }

        return static::$cache[$k1][$k2];
    }
}
