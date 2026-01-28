<?php
/**
 * @template T
 */
interface Container {
    /** @param T $value */
    public function take($value): void;
}

class Foo {
    /**
     * @template T
     * @param Container<T> $c
     */
    function jsonFromEntityCollection(Container $c): void {
        $c->take("foo");
    }
}
