<?php
/**
 * @method static int length(string $length)
 */
class Test {
    public static function __callStatic(string $name, array $args): mixed {
        return match ($name) {
            "length" => strlen((string) $args[0]),
            default => throw new \Error("Undefined method"),
        };
    }
}
$closure = Test::length(...);
$length = $closure("test");
