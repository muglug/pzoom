<?php
class Foo {
    public function bar() : void {}
}

function takesFoo(Foo $foo) : void {}

/** @param mixed $f */
function takesMixed($f) : void {
    if (rand(0, 1)) {
        $f = new Foo();
    }
    /** @psalm-suppress MixedArgument */
    takesFoo($f);
    $f->bar();
}
