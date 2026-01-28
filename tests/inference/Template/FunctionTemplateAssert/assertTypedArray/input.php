<?php
namespace Bar;

class A {
    public function foo() : void {}
}

/**
 * @template T
 * @param class-string<T> $expected
 * @param mixed  $actual
 * @psalm-assert T[] $actual
 */
function assertArrayOf($expected, $actual) : void {}

function bar(array $arr) : void {
    assertArrayOf(A::class, $arr);
    foreach ($arr as $a) {
        $a->foo();
    }
}