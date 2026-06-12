<?php
class Type {
    /**
     * @psalm-assert-if-true FooType $this
     */
    public function assertFoo() : bool {
        return $this instanceof FooType;
    }

    /**
     * @psalm-assert-if-true BarType $this
     */
    public function assertBar() : bool {
        return $this instanceof BarType;
    }
}

interface FooType {
    public function foo(): void;
}

interface BarType {
    public function bar(): void;
}

function takesType(Type $t) : void {
    if ($t->assertFoo() && $t->assertBar()) {
        $t->foo();
        $t->bar();
    }
}
