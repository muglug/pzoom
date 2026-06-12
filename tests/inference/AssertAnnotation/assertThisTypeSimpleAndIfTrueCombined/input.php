<?php
class Type {
    /**
     * @psalm-assert BarType $this
     * @psalm-assert-if-true FooType $this
     */
    public function isFoo() : bool {
        if (!$this instanceof BarType) {
            throw new \Exception();
        }
        return $this instanceof FooType;
    }
}

interface FooType {
    public function foo(): void;
}

interface BarType {
    public function bar(): void;
}

function takesType(Type $t) : void {
    if ($t->isFoo()) {
        $t->foo();
    }
    $t->bar();
}
