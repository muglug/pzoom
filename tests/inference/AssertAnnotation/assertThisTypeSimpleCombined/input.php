<?php
class Type {
    /**
     * @psalm-assert FooType $this
     */
    public function assertFoo() : void {
        if (!$this instanceof FooType) {
            throw new \Exception();
        }
        return;
    }

    /**
     * @psalm-assert BarType $this
     */
    public function assertBar() : void {
        if (!$this instanceof BarType) {
            throw new \Exception();
        }
        return;
    }
}

interface FooType {
    public function foo(): void;
}

interface BarType {
    public function bar(): void;
}

/** @param Type&FooType $t */
function takesType(Type $t) : void {
    $t->assertBar();
    $t->foo();
    $t->bar();
}
