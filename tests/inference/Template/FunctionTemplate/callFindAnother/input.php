<?php
/**
 * @template T as Foo
 * @param T $foo
 * @return T
 */
function loader($foo) {
    return $foo::getAnother();
}

/**
 * @psalm-consistent-constructor
 */
class Foo {
    /** @return static */
    public static function getAnother() {
        return new static();
    }
}