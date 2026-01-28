<?php
namespace FooFoo;

class A {
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public static function foo($x) {
        return $x;
    }
}

function bar(string $a): void { }

bar(A::foo("string"));