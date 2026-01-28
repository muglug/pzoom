<?php
namespace Bar;

class A {
    /** @var class-string-map<T, T> */
    public static array $map = [];

    /**
     * @template T
     * @param class-string<T> $class
     */
    public function get(string $class) : void {
        self::$map[$class] = 5;
    }
}
