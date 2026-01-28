<?php
namespace FooFoo;

class A {
    /**
     * @template T
     * @param T $x
     * @return T
     */
    public function foo($x) {
        return $x;
    }
}

function bar(string $a): void { }

bar((new A())->foo("string"));