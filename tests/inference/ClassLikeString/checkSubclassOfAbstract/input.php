<?php
interface Foo {
    public static function Bar() : bool;
};

class FooClass implements Foo {
    public static function Bar() : bool {
        return true;
    }
}

function foo(string $className) : bool {
    if (is_subclass_of($className, Foo::class, true)) {
        return $className::Bar();
    }

    return false;
}
