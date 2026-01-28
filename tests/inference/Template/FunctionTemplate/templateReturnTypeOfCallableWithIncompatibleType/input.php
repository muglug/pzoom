<?php
class A {}

class B {
    public static function returnsObjectOrNull() : ?A {
        return random_int(0, 1) ? new A() : null;
    }
}


/**
 * @psalm-template T as object
 * @psalm-param callable() : T $callback
 * @psalm-return T
 */
function makeResultSet(callable $callback)
{
    return $callback();
}

makeResultSet([B::class, "returnsObjectOrNull"]);
