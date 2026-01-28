<?php
class A {
    public int $id = 0;
}
class B {
    public function foo() : void {}
}

/**
 * @param array<int, B> $arr
 */
function foo(A $a, array $arr): void {
    if (!isset($arr[$a->id])) {
        $arr[$a->id] = new B();
    }
    $arr[$a->id]->foo();
}