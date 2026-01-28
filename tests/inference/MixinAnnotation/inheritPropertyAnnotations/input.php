<?php
/**
 * @property string $foo
 */
class A {}

/**
 * @mixin A
 */
class B {
    /** @return mixed */
    public function __get(string $s) {
        return 5;
    }
}

function toArray(B $b) : string {
    return $b->foo;
}
