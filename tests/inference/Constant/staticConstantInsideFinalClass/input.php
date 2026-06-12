<?php
final class A {
    public const STRING = "1,2,3";
    public static function foo(): void {
        print_r(explode(",", static::STRING));
    }
}
