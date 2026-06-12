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
    switch (true) {
        case $t->isFoo():
            $t->bar();
    }
}
