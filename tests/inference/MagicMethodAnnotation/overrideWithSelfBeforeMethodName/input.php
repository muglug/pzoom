<?php
class A {
    public static function make(): self {
        return new self();
    }
}

/**
 * @method static self make()
 */
class B extends A {}

function makeB(): B {
    return B::make();
}
