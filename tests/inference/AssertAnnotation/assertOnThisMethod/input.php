<?php
/** @psalm-immutable */
class A {
    private ?array $arr = null;

    public function __construct(?array $arr) {
        $this->arr = $arr;
    }

    /** @psalm-assert-if-true !null $this->getarray() */
    public function hasArray() : bool {
        return $this->arr !== null;
    }

    public function getArray() : ?array {
        return $this->arr;
    }
}

function foo(A $a) : void {
    if (!$a->hasArray()) {
        return;
    }

    echo count($a->getArray());
}
