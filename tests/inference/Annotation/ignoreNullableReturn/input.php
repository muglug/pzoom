<?php
class A {
    /** @var int */
    public $bar = 5;
    public function foo(): void {}
}

/**
 * @return ?A
 * @psalm-ignore-nullable-return
 */
function makeA() {
    return rand(0, 1) ? new A() : null;
}

function takeA(A $_a): void { }

$a = makeA();
$a->foo();
$a->bar = 7;
takeA($a);
