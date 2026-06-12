<?php
/**
 * @psalm-consistent-constructor
 */
abstract class FooBase {
    public final function __construct() {}

    public function baz() : void {
        echo "hello";
    }
}

/**
 * @psalm-template T as FooBase
 * @psalm-param class-string<T> $type
 * @psalm-return T
 */
function createFoo($type): FooBase {
    return new $type();
}

final class Foo extends FooBase {}

createFoo(Foo::class)->baz();
