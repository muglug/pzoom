<?php
class I {
    /**
     * @template T as Foo
     * @param class-string<T> $class
     * @return T|null
     */
    public function loader(string $class) {
        return $class::load();
    }
}

/**
 * @psalm-consistent-constructor
 */
class Foo {
    /** @return static */
    public static function load() {
        return new static();
    }
}

class FooChild extends Foo{}

$a = (new I)->loader(FooChild::class);