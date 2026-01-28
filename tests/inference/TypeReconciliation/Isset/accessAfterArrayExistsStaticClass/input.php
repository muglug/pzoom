<?php
abstract class P {
    const MAP = [
        A::class => 1,
        B::class => 2,
        C::class => 3,
    ];

    public function foo(string $s) : int {
        if (!isset(self::MAP[static::class])) {
            throw new \Exception("bad");
        }
        return self::MAP[static::class];
    }
}

class A extends P {}
class B extends P {}
class C extends P {}