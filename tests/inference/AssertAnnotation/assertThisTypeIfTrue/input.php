<?php
class Type {
    /**
     * @psalm-assert-if-true FooType $this
     */
    public function isFoo() : bool {
        return $this instanceof FooType;
    }
}

class FooType extends Type {
    public function bar(): void {}
}

function takesType(Type $t) : void {
    if ($t->isFoo()) {
        $t->bar();
    }
}
