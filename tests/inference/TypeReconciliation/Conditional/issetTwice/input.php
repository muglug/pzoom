<?php
class B {
    public function foo() : bool {
        return true;
    }
}

/** @param array<int, B> $p */
function foo(array $p, int $id) : void {
    if ((isset($p[$id]) && rand(0, 1))
        || (!isset($p[$id]) && rand(0, 1))
    ) {
        isset($p[$id]) ? $p[$id] : new B;
        isset($p[$id]) ? $p[$id]->foo() : "bar";
    }
}