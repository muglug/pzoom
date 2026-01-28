<?php
class C {}
class D {}

class A {
    const FLAGS = [
        0 => [C::class => "foo"],
        1 => [D::class => "bar"],
    ];

    private function foo(int $i) : void {
        if (array_key_exists(C::class, self::FLAGS[$i])) {
            echo self::FLAGS[$i][C::class];
        }
    }
}