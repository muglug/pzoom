<?php
interface Foo {}

/**
 * @template T as Foo
 * @param class-string<T> $fooClass
 * @return T
 */
function get($fooClass, Foo $foo) {
    if ($foo instanceof $fooClass) {
        return $foo;
    }

    throw new \Exception();
}