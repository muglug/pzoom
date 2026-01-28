<?php
interface Foo {
    public static function one() : void;
};

interface Bar {
    public static function two() : void;
}

/**
 * @param interface-string<Foo&Bar> $className
 */
function foo($className) : void {
    $className::one();
    $className::two();
}
