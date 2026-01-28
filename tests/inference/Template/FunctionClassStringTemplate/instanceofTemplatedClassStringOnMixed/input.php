<?php
interface Foo {}

/**
 * @template T as Foo
 * @param class-string<T> $fooClass
 * @param mixed $foo
 * @return T
 */
function get($fooClass, $foo) {
    if ($foo instanceof $fooClass) {
        return $foo;
    }

    throw new \Exception();
}