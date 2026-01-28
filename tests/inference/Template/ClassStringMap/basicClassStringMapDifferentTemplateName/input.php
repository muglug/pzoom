<?php
namespace Bar;

/**
 * @psalm-consistent-constructor
 */
class Foo {}

class A {
    /** @var class-string-map<T as Foo, T> */
    public static array $map = [];

    /**
     * @template U as Foo
     * @param class-string<U> $class
     * @return U
     */
    public function get(string $class) : Foo {
        if (isset(self::$map[$class])) {
            return self::$map[$class];
        }

        self::$map[$class] = new $class();
        return self::$map[$class];
    }
}