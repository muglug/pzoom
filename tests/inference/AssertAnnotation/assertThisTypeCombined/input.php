<?php
class Type {
    /**
     * @psalm-assert FooType $this
     */
    public function assertFoo() : void {
        if (!$this instanceof FooType) {
            throw new \Exception();
        }
    }

    /**
     * @psalm-assert BarType $this
     */
    public function assertBar() : void {
        if (!$this instanceof BarType) {
            throw new \Exception();
        }
    }
}

interface FooType {
    public function foo(): void;
}

interface BarType {
    public function bar(): void;
}

function takesType(Type $t) : void {
    $t->assertFoo();
    $t->assertBar();
    $t->foo();
    $t->bar();
}
