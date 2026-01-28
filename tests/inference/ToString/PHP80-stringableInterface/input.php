<?php
interface Foo extends Stringable {}

function takesString(string $s) : void {}

function takesFoo(Foo $foo) : void {
    /** @psalm-suppress ImplicitToStringCast */
    takesString($foo);
}

class FooImplementer implements Foo {
    public function __toString() : string {
        return "hello";
    }
}

takesFoo(new FooImplementer());
