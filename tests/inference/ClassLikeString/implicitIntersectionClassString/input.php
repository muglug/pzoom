<?php
interface Foo {
    public static function one() : bool;
};

interface Bar {
    public static function two() : bool;
}

/**
 * @param interface-string<Bar> $className
 */
function foo(string $className) : void {
    $className::two();

    if (is_subclass_of($className, Foo::class, true)) {
        $className::one();
        $className::two();
    }
}
